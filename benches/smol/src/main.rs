use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use beakid::{BeakId, Generator};

fn main() {
    println!("beakid smol generator benchmark");
    println!();
    println!(
        "{:<14} {:>6} {:>12} {:>14} {:>10} {:>9} {:>12}",
        "case", "tasks", "total ids", "ids/s", "ns/id", "virt-win", "unique"
    );
    println!("{}", "-".repeat(81));

    bench_run(1, 100_000);
    bench_run(2, 2_000_000);
    bench_run(4, 100_000);
    bench_run(8, 100_000);
    bench_run(28, 100_600_000);
    bench_run(56, 5_600_000);
}

fn bench_run(tasks: usize, total_ids: u64) {
    let ops_per_task = total_ids / tasks as u64;

    let spin = smol::block_on(run_case("spin", tasks, ops_per_task, false));
    print_result(&spin);

    let yield_ = smol::block_on(run_case("smol-yield", tasks, ops_per_task, true));
    print_result(&yield_);

    if spin.duplicates > 0 || yield_.duplicates > 0 {
        eprintln!("\nFAIL: duplicate IDs detected");
        std::process::exit(1);
    }
}

fn print_result(r: &BenchResult) {
    let unique_col = if r.duplicates == 0 {
        "ok".to_string()
    } else {
        format!("FAIL ({} dup)", r.duplicates)
    };
    println!(
        "{:<14} {:>6} {:>12} {:>14.0} {:>10.2} {:>9} {:>12}",
        r.name,
        r.tasks,
        r.total_ids,
        r.ids_per_second(),
        r.ns_per_id(),
        r.virtual_windows_used(),
        unique_col,
    );
}

struct BenchResult {
    name: &'static str,
    tasks: usize,
    total_ids: u64,
    elapsed: Duration,
    duplicates: u64,
}

impl BenchResult {
    fn ids_per_second(&self) -> f64 {
        self.total_ids as f64 / self.elapsed.as_secs_f64()
    }

    fn ns_per_id(&self) -> f64 {
        self.elapsed.as_nanos() as f64 / self.total_ids as f64
    }

    fn virtual_windows_used(&self) -> u64 {
        const IDS_PER_WINDOW: u64 = 1 << 19;
        let windows_consumed = self.total_ids / IDS_PER_WINDOW;
        let real_windows = self.elapsed.as_millis() as u64 / 100;
        windows_consumed.saturating_sub(real_windows)
    }
}

async fn run_case(
    name: &'static str,
    tasks: usize,
    ops_per_task: u64,
    smol_yield: bool,
) -> BenchResult {
    let (generator, _handle) = beakid::smol_run!(0, SystemTime::UNIX_EPOCH);

    smol::future::yield_now().await;
    smol::Timer::after(Duration::from_millis(1)).await;

    let start = Instant::now();
    let mut handles = Vec::with_capacity(tasks);

    for _ in 0..tasks {
        let g = Arc::clone(&generator);
        handles.push(smol::spawn(async move {
            generate_many(g, ops_per_task, smol_yield).await
        }));
    }

    let mut all_ids: Vec<BeakId> = Vec::with_capacity(tasks * ops_per_task as usize);
    for handle in handles {
        all_ids.extend(handle.await);
    }

    let elapsed = start.elapsed();
    let total_ids = all_ids.len() as u64;

    all_ids.sort_unstable();
    let duplicates = all_ids.windows(2).filter(|w| w[0] == w[1]).count() as u64;

    BenchResult {
        name,
        tasks,
        total_ids,
        elapsed,
        duplicates,
    }
}

async fn generate_many(
    generator: Arc<Generator>,
    ops_per_task: u64,
    smol_yield: bool,
) -> Vec<BeakId> {
    let mut ids = Vec::with_capacity(ops_per_task as usize);

    if smol_yield {
        for _ in 0..ops_per_task {
            let id = beakid::smol_generate!(generator);
            ids.push(black_box(id));
        }
    } else {
        for _ in 0..ops_per_task {
            ids.push(black_box(generator.must_generate()));
        }
    }

    ids
}
