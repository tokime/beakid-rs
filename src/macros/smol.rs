/// En: Creates an `Arc<Generator>` and starts a smol background task that calls
/// [`update_time`](crate::Generator::update_time) every 100 ms.
///
/// The first `update_time` call is made immediately so the generator is ready
/// to produce IDs as soon as the macro returns.
///
/// Returns a tuple `(Arc<Generator>, Task<()>)`. The handle can be used to stop
/// the background job: dropping the `Task` **cancels it immediately**, or call
/// `handle.cancel().await` for a graceful async cancellation.
/// Keep the handle alive (e.g. in a variable) for as long as the generator is in use.
///
/// **Arguments**
/// - `$worker_id` — `u64` in `0..=1023`; must be unique across all `Generator`
///   instances running simultaneously in your deployment.
/// - `$epoch` — [`SystemTime`](std::time::SystemTime) reference point for timestamps.
///
/// **Returns** `(Arc<`[`Generator`](crate::Generator)`>, smol::Task<()>)`
///
/// # Panics
///
/// Panics if `worker_id >= 1024`. Requires the smol global executor to be running.
///
/// Ru: Создаёт `Arc<Generator>` и запускает фоновую задачу smol, которая вызывает
/// [`update_time`](crate::Generator::update_time) каждые 100 мс.
///
/// Первый вызов `update_time` происходит немедленно, поэтому генератор готов
/// выдавать ID сразу после возврата макроса.
///
/// Возвращает кортеж `(Arc<Generator>, Task<()>)`. Дескриптор позволяет
/// остановить фоновую задачу: дроп `Task` **немедленно отменяет её**, либо
/// вызовите `handle.cancel().await` для асинхронной отмены.
/// Держите дескриптор живым (например, в переменной) всё время пока генератор используется.
///
/// **Аргументы**
/// - `$worker_id` — `u64` в диапазоне `0..=1023`; должен быть уникален среди всех
///   экземпляров `Generator`, работающих одновременно в вашем деплое.
/// - `$epoch` — [`SystemTime`](std::time::SystemTime), точка отсчёта для временных меток.
///
/// **Возвращает** `(Arc<`[`Generator`](crate::Generator)`>, smol::Task<()>)`
///
/// # Паника
///
/// Паникует если `worker_id >= 1024`. Требует запущенного глобального исполнителя smol.
#[macro_export]
macro_rules! smol_run {
    ($worker_id:expr, $epoch:expr $(,)?) => {{
        let generator = ::std::sync::Arc::new($crate::Generator::new($worker_id, $epoch));

        generator.update_time();

        let updater = ::std::sync::Arc::clone(&generator);
        let handle = ::smol::spawn(async move {
            loop {
                ::smol::Timer::after(::std::time::Duration::from_millis(100)).await;
                updater.update_time();
            }
        });

        (generator, handle)
    }};
}

/// En: Generates a unique ID inside a smol async context, yielding to the
/// executor on each retry.
///
/// On [`Error::StateUpdating`](crate::Error::StateUpdating) or
/// [`Error::Blocked`](crate::Error::Blocked) the macro calls
/// `smol::future::yield_now().await`, allowing other tasks — including the
/// background `update_time` job — to make progress before retrying.
///
/// **Arguments**
/// - `$generator` — an expression that evaluates to `Arc<`[`Generator`](crate::Generator)`>`
///   or any type that derefs to `Generator`.
///
/// **Returns** [`BeakId`](crate::BeakId)
///
/// Must be used inside a `smol` async context with `.await` support.
///
/// Ru: Генерирует уникальный ID внутри async-контекста smol, уступая исполнителю
/// при каждой повторной попытке.
///
/// При [`Error::StateUpdating`](crate::Error::StateUpdating) или
/// [`Error::Blocked`](crate::Error::Blocked) макрос вызывает
/// `smol::future::yield_now().await`, позволяя другим задачам — включая фоновую
/// задачу `update_time` — продвинуться перед повторной попыткой.
///
/// **Аргументы**
/// - `$generator` — выражение, результатом которого является `Arc<`[`Generator`](crate::Generator)`>`
///   или любой тип, разыменовываемый в `Generator`.
///
/// **Возвращает** [`BeakId`](crate::BeakId)
///
/// Должен использоваться внутри async-контекста `smol` с поддержкой `.await`.
#[macro_export]
macro_rules! smol_generate {
    ($generator:expr) => {{
        loop {
            match $generator.generate() {
                Ok(id) => break id,
                Err(_) => ::smol::future::yield_now().await,
            }
        }
    }};
}
