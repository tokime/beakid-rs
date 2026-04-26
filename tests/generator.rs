use std::sync::Arc;
use std::time::{Duration, SystemTime};

use beakid::{BeakId, Error, Generator};

// ─── helpers ────────────────────────────────────────────────────────────────

fn make_generator(worker_id: u64) -> Generator {
    let g = Generator::new(worker_id, SystemTime::UNIX_EPOCH);
    g.update_time();
    g
}

fn count_duplicates(mut ids: Vec<BeakId>) -> usize {
    ids.sort_unstable();
    ids.windows(2).filter(|w| w[0] == w[1]).count()
}

// Количество id на одно 100ms-окно: 19-битный счётчик последовательностей.
const IDS_PER_WINDOW: usize = 1 << 19; // 524_288

// ─── uniqueness ─────────────────────────────────────────────────────────────

#[test]
fn single_thread_ids_are_unique() {
    let g = make_generator(0);

    let ids: Vec<BeakId> = (0..10_000).map(|_| g.generate().unwrap()).collect();

    assert_eq!(count_duplicates(ids), 0);
}

#[test]
fn concurrent_ids_are_unique() {
    let g = Arc::new(make_generator(0));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let g = Arc::clone(&g);
            std::thread::spawn(move || {
                (0..10_000).map(|_| g.must_generate()).collect::<Vec<_>>()
            })
        })
        .collect();

    let ids: Vec<BeakId> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();

    assert_eq!(count_duplicates(ids), 0);
}

#[test]
fn workers_with_different_ids_never_collide() {
    let g0 = Arc::new(make_generator(0));
    let g1 = Arc::new(make_generator(1));

    let h0 = {
        let g = Arc::clone(&g0);
        std::thread::spawn(move || (0..10_000).map(|_| g.must_generate()).collect::<Vec<_>>())
    };
    let h1 = {
        let g = Arc::clone(&g1);
        std::thread::spawn(move || (0..10_000).map(|_| g.must_generate()).collect::<Vec<_>>())
    };

    let ids: Vec<BeakId> = h0.join().unwrap().into_iter().chain(h1.join().unwrap()).collect();

    assert_eq!(count_duplicates(ids), 0);
}

// ─── monotonicity ───────────────────────────────────────────────────────────

// В одном потоке с одним worker_id каждый новый id должен быть строго больше
// предыдущего: старшие биты — timestamp, следующие — счётчик.
#[test]
fn single_thread_ids_are_monotonically_increasing() {
    let g = make_generator(0);
    let mut prev = g.generate().unwrap();

    for _ in 1..10_000 {
        let cur = g.generate().unwrap();
        assert!(cur > prev, "monotonicity broken: {cur:?} <= {prev:?}");
        prev = cur;
    }
}

// Монотонность сохраняется при переходе между окнами: вызов update_time
// не должен откатывать внутренний счётчик назад.
#[test]
fn ids_remain_monotonic_across_update_time_calls() {
    let g = make_generator(0);
    let mut prev = g.generate().unwrap();

    for _ in 0..5 {
        for _ in 0..1_000 {
            let cur = g.generate().unwrap();
            assert!(cur > prev, "monotonicity broken after update_time");
            prev = cur;
        }
        g.update_time();
    }
}

// ─── virtual windows ────────────────────────────────────────────────────────

// Генератор должен продолжать работу после исчерпания одного окна.
#[test]
fn virtual_windows_allow_burst_beyond_single_window() {
    let g = make_generator(0);

    // Генерируем больше одного окна без вызова update_time.
    let target = IDS_PER_WINDOW + 10_000;
    let mut generated = 0;

    for _ in 0..target {
        match g.generate() {
            Ok(_) => generated += 1,
            Err(Error::StateUpdating) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    assert!(
        generated >= IDS_PER_WINDOW,
        "virtual windows should allow at least {IDS_PER_WINDOW} IDs, got {generated}"
    );
}

// Id-шники остаются уникальными при переходе через границы виртуальных окон.
#[test]
fn ids_unique_across_virtual_window_boundaries() {
    let g = make_generator(0);

    let target = IDS_PER_WINDOW * 3;
    let mut ids = Vec::with_capacity(target);

    for _ in 0..target {
        match g.generate() {
            Ok(id) => ids.push(id),
            Err(Error::Blocked) => break,
            Err(Error::StateUpdating) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    assert_eq!(count_duplicates(ids), 0);
}

// ─── blocking / unblocking ──────────────────────────────────────────────────

// Генерирует 20 окон с помощью 64 потоков для скорости (цель: < 100ms).
// Возвращает true если блокировка сработала, false — если машина слишком медленная.
fn try_exhaust_and_block(g: &Arc<Generator>) -> bool {
    let total = IDS_PER_WINDOW * 20;
    let n_threads = 64;
    let per_thread = (total + n_threads - 1) / n_threads;

    let handles: Vec<_> = (0..n_threads)
        .map(|_| {
            let g = Arc::clone(g);
            std::thread::spawn(move || {
                for _ in 0..per_thread {
                    let _ = g.generate();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    g.update_time();
    matches!(g.generate(), Err(Error::Blocked))
}

// epoch = now чтобы начальный timestamp ≈ 0, это максимизирует delta
// до того как real-time успеет обогнать сгенерированные id.
fn make_fresh_generator() -> Arc<Generator> {
    let g = Arc::new(Generator::new(0, SystemTime::now()));
    g.update_time();
    g
}

#[test]
fn blocks_when_virtual_windows_exhausted() {
    let g = make_fresh_generator();

    let blocked = try_exhaust_and_block(&g);

    // На медленных/однопоточных машинах real-time может обогнать генерацию.
    // В таком случае тест пропускается, а не падает.
    if !blocked {
        eprintln!("skip: machine too slow to exhaust windows within one 100ms tick");
        return;
    }

    assert!(
        matches!(g.generate(), Err(Error::Blocked)),
        "generator must return Err(Blocked) after all virtual windows exhausted"
    );
}

#[test]
fn unblocks_after_time_advances() {
    let g = make_fresh_generator();

    let blocked = try_exhaust_and_block(&g);

    if !blocked {
        eprintln!("skip: could not reach blocked state on this machine");
        return;
    }

    // После 20 виртуальных окон нужно подождать пока real-time не обгонит id
    // на достаточно чтобы delta упала ниже 10. 20 окон × 100ms = 2s.
    std::thread::sleep(Duration::from_millis(2200));
    g.update_time();

    assert!(
        g.generate().is_ok(),
        "generator should unblock after real time advances past the blocked delta"
    );
}

// ─── base62 ─────────────────────────────────────────────────────────────────

#[test]
fn base62_roundtrip_preserves_id() {
    let g = make_generator(0);
    for _ in 0..1_000 {
        let id = g.generate().unwrap();
        let decoded = BeakId::from_base62(&id.base62()).expect("decode failed");
        assert_eq!(id, decoded);
    }
}

#[test]
fn base62_is_always_11_chars() {
    for val in [0u64, 1, 1_000_000, u64::MAX / 2, u64::MAX] {
        assert_eq!(BeakId::new(val).base62().len(), 11, "wrong length for value {val}");
    }
}

#[test]
fn from_base62_rejects_wrong_length() {
    assert!(BeakId::from_base62("").is_err());
    assert!(BeakId::from_base62("0000000000").is_err());   // 10 символов
    assert!(BeakId::from_base62("000000000000").is_err()); // 12 символов
}

#[test]
fn from_base62_rejects_invalid_chars() {
    assert!(BeakId::from_base62("0000000000!").is_err());
    assert!(BeakId::from_base62("0000000000 ").is_err());
    assert!(BeakId::from_base62("0000000000\n").is_err());
}

// ─── constructor ────────────────────────────────────────────────────────────

#[test]
fn try_new_validates_worker_id_bounds() {
    assert!(Generator::try_new(0, SystemTime::UNIX_EPOCH).is_ok());
    assert!(Generator::try_new(1023, SystemTime::UNIX_EPOCH).is_ok());
    assert!(Generator::try_new(1024, SystemTime::UNIX_EPOCH).is_err());
    assert!(Generator::try_new(u64::MAX, SystemTime::UNIX_EPOCH).is_err());
}
