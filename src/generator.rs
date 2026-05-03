use crate::{BeakId, Error, states};
use std::sync::atomic::Ordering;
use std::{sync::atomic::AtomicU64, time::SystemTime};

pub(crate) const TIMESTAMP_SHIFT: u8 = 29;
const TIMESTAMP_MASK: u64 = !((1 << TIMESTAMP_SHIFT) - 1);
const WORKER_ID_SHIFT: u8 = 10;
const WORKER_ID_MASK: u64 = (1 << WORKER_ID_SHIFT) - 1;
const INCR: u64 = 1 << WORKER_ID_SHIFT;

/// En: Lock-free unique ID generator based on the Snowflake algorithm.
///
/// Each ID is a 64-bit integer with the following layout:
/// ```text
/// [63..29] timestamp  — 35 bits, units of 100 ms since epoch
/// [28..10] sequence   — 19 bits, up to 524 288 IDs per window
/// [9..0]   worker_id  — 10 bits, distinguishes generator instances
/// ```
///
/// IDs are monotonically increasing within a single thread and unique
/// across all threads sharing the same `Generator` instance.
///
/// [`generate`](Generator::generate) calls [`update_time`](Generator::update_time)
/// automatically at every virtual window boundary (every 524 288 IDs), so the
/// generator can advance and unblock itself under high load without relying solely
/// on the background job. A background job calling
/// [`update_time`](Generator::update_time) every 100 ms is still recommended to
/// keep the timestamp current at low load. Use the
/// [`tokio_run!`](crate::tokio_run) or [`smol_run!`](crate::smol_run) macros to
/// set this up automatically.
///
/// Ru: Потокобезопасный генератор уникальных ID без блокировок, основан на алгоритме Snowflake.
///
/// Каждый ID — 64-битное целое со следующей раскладкой битов:
/// ```text
/// [63..29] timestamp  — 35 бит, единица — 100 мс от epoch
/// [28..10] sequence   — 19 бит, до 524 288 ID на окно
/// [9..0]   worker_id  — 10 бит, различает экземпляры генераторов
/// ```
///
/// ID монотонно возрастают в рамках одного потока и уникальны
/// для всех потоков, разделяющих один экземпляр `Generator`.
///
/// [`generate`](Generator::generate) автоматически вызывает
/// [`update_time`](Generator::update_time) на каждой границе виртуального окна
/// (каждые 524 288 ID), поэтому при высокой нагрузке генератор может продвигаться
/// и разблокировать себя самостоятельно, не полагаясь исключительно на фоновую задачу.
/// Фоновая задача, вызывающая [`update_time`](Generator::update_time) каждые 100 мс,
/// всё равно рекомендуется для поддержания актуального timestamp при низкой нагрузке.
/// Используйте макросы [`tokio_run!`](crate::tokio_run) или [`smol_run!`](crate::smol_run)
/// для автоматической настройки.
pub struct Generator {
    id: AtomicU64,
    state: AtomicU64,
    epoch: SystemTime,
}

impl Generator {
    /// En: Creates a new `Generator`, returning an error if `worker_id` is out of range.
    ///
    /// - `worker_id` — a value in `0..=1023` that uniquely identifies this generator
    ///   instance within your deployment. Two generators running simultaneously
    ///   (different processes, machines, or threads using separate instances) **must**
    ///   have different `worker_id`s to guarantee globally unique IDs.
    /// - `epoch` — the reference point for timestamps. All generated IDs encode time
    ///   elapsed since this moment. Use a fixed date (e.g. `SystemTime::UNIX_EPOCH`
    ///   or your service launch date) — changing it invalidates sort order of existing IDs.
    ///
    /// Returns [`Error::InvalidWorkerId`] if `worker_id >= 1024`.
    ///
    /// The generator is not ready to produce IDs until [`update_time`](Generator::update_time)
    /// is called at least once.
    ///
    /// Ru: Создаёт новый `Generator`, возвращая ошибку если `worker_id` вне допустимого диапазона.
    ///
    /// - `worker_id` — значение в диапазоне `0..=1023`, уникально идентифицирующее данный
    ///   экземпляр генератора в рамках деплоя. Два одновременно работающих генератора
    ///   (разные процессы, машины или потоки с разными экземплярами) **обязаны** иметь
    ///   разные `worker_id`, чтобы гарантировать глобальную уникальность ID.
    /// - `epoch` — точка отсчёта для временных меток. Все генерируемые ID кодируют время,
    ///   прошедшее с этого момента. Используйте фиксированную дату (например,
    ///   `SystemTime::UNIX_EPOCH` или дату запуска сервиса) — её изменение нарушает
    ///   порядок сортировки уже существующих ID.
    ///
    /// Возвращает [`Error::InvalidWorkerId`] если `worker_id >= 1024`.
    ///
    /// Генератор не готов выдавать ID пока не вызван [`update_time`](Generator::update_time)
    /// хотя бы один раз.
    pub fn try_new(worker_id: u64, epoch: SystemTime) -> Result<Self, Error> {
        if worker_id >= 1024 {
            return Err(Error::InvalidWorkerId);
        }

        Ok(Self {
            id: AtomicU64::new(worker_id),
            state: AtomicU64::new(states::NOT_INITED),
            epoch,
        })
    }

    /// En: Creates a new `Generator`.
    ///
    /// Identical to [`try_new`](Generator::try_new) but panics instead of returning an error.
    ///
    /// # Panics
    ///
    /// Panics if `worker_id >= 1024`.
    ///
    /// Ru: Создаёт новый `Generator`.
    ///
    /// Идентичен [`try_new`](Generator::try_new), но паникует вместо возврата ошибки.
    ///
    /// # Паника
    ///
    /// Паникует если `worker_id >= 1024`.
    pub fn new(worker_id: u64, epoch: SystemTime) -> Self {
        Self::try_new(worker_id, epoch).expect("worker_id out of range (>=1024)")
    }

    /// En: Generates a unique ID, spinning until one is available.
    ///
    /// On [`Error::Blocked`] calls [`update_time`](Generator::update_time) and spins,
    /// allowing the generator to unblock itself once real time has advanced sufficiently.
    /// Suitable for synchronous contexts where a blocking spin is acceptable.
    ///
    /// Prefer [`tokio_generate!`](crate::tokio_generate) or [`smol_generate!`](crate::smol_generate)
    /// in async contexts to avoid blocking the executor thread.
    ///
    /// # Panics
    ///
    /// Panics if [`update_time`](Generator::update_time) has never been called
    /// (generator not initialised).
    ///
    /// Ru: Генерирует уникальный ID, крутясь в spinloop до тех пор пока ID не будет доступен.
    ///
    /// При [`Error::Blocked`] вызывает [`update_time`](Generator::update_time) и спинит,
    /// позволяя генератору разблокировать себя, когда реальное время достаточно продвинулось.
    /// Подходит для синхронных контекстов, где блокирующий spin допустим.
    ///
    /// В async-контекстах предпочтительнее [`tokio_generate!`](crate::tokio_generate)
    /// или [`smol_generate!`](crate::smol_generate), чтобы не блокировать поток исполнителя.
    ///
    /// # Паника
    ///
    /// Паникует если [`update_time`](Generator::update_time) ни разу не вызывался
    /// (генератор не инициализирован).
    pub fn must_generate(&self) -> BeakId {
        loop {
            match self.generate() {
                Ok(id) => return id,
                Err(Error::Blocked) => {
                    self.update_time();
                    std::thread::yield_now();
                }
                Err(_) => unreachable!(),
            }
        }
    }

    /// En: Attempts to generate a unique ID, returning an error only if the generator
    /// is blocked.
    ///
    /// On success the returned [`BeakId`] is guaranteed to be unique across all
    /// concurrent callers sharing this `Generator` instance, and greater than any
    /// ID previously returned on the same thread.
    ///
    /// If [`update_time`](Generator::update_time) is running concurrently
    /// ([`StateUpdating`](crate::Error::StateUpdating)), `generate` spins with
    /// [`spin_loop`](std::hint::spin_loop) until the update finishes — this state
    /// lasts nanoseconds and is not worth surfacing to the caller.
    ///
    /// When the sequence counter crosses a virtual window boundary (every 524 288 IDs),
    /// `generate` calls [`update_time`](Generator::update_time) inline to check whether
    /// real time has caught up.
    ///
    /// # Errors
    ///
    /// - [`Error::Blocked`] — all 10 virtual time windows are exhausted and real time
    ///   has not yet caught up; the generator will unblock on the next
    ///   [`update_time`](Generator::update_time) call or at the next window boundary.
    ///
    /// # Panics
    ///
    /// Panics if [`update_time`](Generator::update_time) has never been called
    /// (generator not initialised). Use the [`tokio_run!`](crate::tokio_run) or
    /// [`smol_run!`](crate::smol_run) macros to avoid this.
    ///
    /// Ru: Пытается сгенерировать уникальный ID, возвращая ошибку только если генератор
    /// заблокирован.
    ///
    /// При успехе возвращённый [`BeakId`] гарантированно уникален среди всех
    /// конкурентных вызовов на этом экземпляре `Generator` и больше любого ID,
    /// возвращённого ранее в том же потоке.
    ///
    /// Если [`update_time`](Generator::update_time) выполняется параллельно
    /// ([`StateUpdating`](crate::Error::StateUpdating)), `generate` крутится в
    /// [`spin_loop`](std::hint::spin_loop) до завершения обновления — это состояние
    /// длится наносекунды и не имеет смысла пробрасывать его вызывающему.
    ///
    /// Когда счётчик последовательности пересекает границу виртуального окна
    /// (каждые 524 288 ID), `generate` вызывает [`update_time`](Generator::update_time)
    /// inline, чтобы проверить — успело ли реальное время догнать виртуальное.
    ///
    /// # Ошибки
    ///
    /// - [`Error::Blocked`] — исчерпаны все 10 виртуальных временных окон и реальное
    ///   время ещё не догнало виртуальное; генератор разблокируется при следующем вызове
    ///   [`update_time`](Generator::update_time) или на следующей границе окна.
    ///
    /// # Паника
    ///
    /// Паникует если [`update_time`](Generator::update_time) ни разу не вызывался
    /// (генератор не инициализирован). Используйте макросы [`tokio_run!`](crate::tokio_run)
    /// или [`smol_run!`](crate::smol_run) чтобы избежать этого.
    pub fn generate(&self) -> Result<BeakId, Error> {
        let state = loop {
            let s = self.state.load(Ordering::Acquire);
            if s & states::UPDATING == 0 {
                break s;
            }
            std::hint::spin_loop();
        };

        if state == states::NOT_INITED {
            panic!(
                "Run worker with update_time with 100ms interval, or use macro for create generator"
            )
        }

        if state & states::BLOCKED > 0 {
            return Err(Error::Blocked);
        }

        let id = self.id.fetch_add(INCR, Ordering::Relaxed);

        // Если инкремент обновляет время
        // То есть уходим в виртуальное время
        // Вызываем update_time для проверки
        let new_id = id + INCR;
        if (id & TIMESTAMP_MASK) != (new_id & TIMESTAMP_MASK) {
            self.update_time();
        }

        Ok(BeakId::from(id))
    }

    /// En: Advances the internal timestamp and manages virtual window state.
    ///
    /// Must be called **every 100 ms** by a background job for the generator to
    /// function correctly. The first call initialises the generator; subsequent
    /// calls keep the timestamp current.
    ///
    /// **Virtual windows:** each 100 ms window holds up to 524 288 IDs. If the
    /// sequence counter overflows into the next window before real time catches up,
    /// the generator borrows that window (up to 10 in total). On this call, if the
    /// borrowed window count is within the limit, generation continues uninterrupted.
    /// If all 10 virtual windows are exhausted, the generator transitions to
    /// [`Error::Blocked`] until a future call detects that real time has advanced
    /// far enough to reduce the borrow count below 10.
    ///
    /// This method is concurrency-safe: if two calls race, the second one exits
    /// immediately without corrupting state.
    ///
    /// Use [`tokio_run!`](crate::tokio_run) or [`smol_run!`](crate::smol_run) to
    /// schedule this automatically instead of calling it manually.
    ///
    /// # Panics
    ///
    /// Panics if the current system time is earlier than `epoch`.
    ///
    /// Ru: Продвигает внутренний timestamp и управляет состоянием виртуальных окон.
    ///
    /// Должен вызываться **каждые 100 мс** фоновой задачей для корректной работы
    /// генератора. Первый вызов инициализирует генератор; последующие удерживают
    /// timestamp актуальным.
    ///
    /// **Виртуальные окна:** каждое 100 мс-окно вмещает до 524 288 ID. Если счётчик
    /// последовательности переполняется в следующее окно до того как реальное время
    /// успевает продвинуться, генератор «занимает» это окно (до 10 штук суммарно).
    /// При данном вызове, если число заимствованных окон в пределах лимита, генерация
    /// продолжается без прерываний. Если исчерпаны все 10 виртуальных окон, генератор
    /// переходит в состояние [`Error::Blocked`] до тех пор пока следующий вызов не
    /// обнаружит, что реальное время продвинулось достаточно чтобы снизить счётчик
    /// заимствований ниже 10.
    ///
    /// Метод потокобезопасен: если два вызова гонятся, второй немедленно выходит
    /// не повреждая состояние.
    ///
    /// Используйте [`tokio_run!`](crate::tokio_run) или [`smol_run!`](crate::smol_run)
    /// для автоматического планирования вместо ручного вызова.
    ///
    /// # Паника
    ///
    /// Паникует если текущее системное время раньше чем `epoch`.
    pub fn update_time(&self) {
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            // Идёт обновление в другом потоке
            // По идеи такого не должно быть
            // Но допустим джоба СИЛЬНО задержалась
            if state & states::UPDATING > 0 {
                return;
            }

            // Если заблокировано, то разблокировать
            // По идеи, если заблокировало в предыдущий проход
            // то уже прошло 100ms и началось новое окно
            if state & states::BLOCKED > 0 {
                let timestamp = ((SystemTime::now()
                    .duration_since(self.epoch)
                    .expect("Current time before epoch")
                    .as_millis() as u64)
                    / 100)
                    << TIMESTAMP_SHIFT;
                let id = self.id.load(Ordering::Relaxed);
                if id > timestamp {
                    let delta = (id - timestamp) >> TIMESTAMP_SHIFT;
                    // Это важно. Разблокировка только, когда виртуальных окон 8. Чтобы виртуальное
                    // время далено не убегало
                    if delta >= 8 {
                        return;
                    }
                }
                self.state.store(0, Ordering::Release);
                return;
            }

            // Пытаемся заблокировать
            match self.state.compare_exchange_weak(
                state,
                states::UPDATING | states::BLOCKED,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(cur) => state = cur,
            }
        }

        let now = SystemTime::now();
        let mut id = self.id.load(Ordering::Relaxed);

        let timestamp = ((now
            .duration_since(self.epoch)
            .expect("Current time before epoch")
            .as_millis() as u64)
            / 100)
            << TIMESTAMP_SHIFT;

        // Обновляем в цикле
        // На случай если другой поток сделает инкремент id
        loop {
            // Если id больше timestamp, то
            // мы либо в том же окне, либо id уже на виртуальном времени.
            if id > timestamp {
                // Вычесляем дельту и сдвигаем
                let delta = (id - timestamp) >> TIMESTAMP_SHIFT;

                // Виртуальные окна ещё не исчерпаны -> выходим
                if delta < 10 {
                    break;
                }

                self.state.store(states::BLOCKED, Ordering::Release);
                return;
            }

            let new_id = (id & WORKER_ID_MASK) | timestamp;
            match self
                .id
                .compare_exchange_weak(id, new_id, Ordering::Release, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(cur) => id = cur,
            }
        }

        self.state.store(0, Ordering::Release);
    }

    /// En: Returns created_at from BeakId
    ///
    /// Ru: Возвращает created_at из BeakId
    pub fn get_created_at(&self, id: &BeakId) -> u64 {
        id.timestamp()
            + self
                .epoch
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("epoch before unix epoch")
                .as_millis() as u64
    }
}
