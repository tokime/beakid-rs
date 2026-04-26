/// En: Creates an `Arc<Generator>` and starts a tokio background task that calls
/// [`update_time`](crate::Generator::update_time) every 100 ms.
///
/// The first `update_time` call is made immediately so the generator is ready
/// to produce IDs as soon as the macro returns.
///
/// Returns a tuple `(Arc<Generator>, JoinHandle<()>)`. The handle can be used
/// to stop the background job: calling `handle.abort()` cancels the task.
/// Dropping the handle **does not** stop the task — it keeps running until
/// explicitly aborted or the runtime shuts down.
///
/// **Arguments**
/// - `$worker_id` — `u64` in `0..=1023`; must be unique across all `Generator`
///   instances running simultaneously in your deployment.
/// - `$epoch` — [`SystemTime`](std::time::SystemTime) reference point for timestamps.
///
/// **Returns** `(Arc<`[`Generator`](crate::Generator)`>, JoinHandle<()>)`
///
/// # Panics
///
/// Panics if `worker_id >= 1024`. Must be called inside a tokio async context.
///
/// Ru: Создаёт `Arc<Generator>` и запускает фоновую задачу tokio, которая вызывает
/// [`update_time`](crate::Generator::update_time) каждые 100 мс.
///
/// Первый вызов `update_time` происходит немедленно, поэтому генератор готов
/// выдавать ID сразу после возврата макроса.
///
/// Возвращает кортеж `(Arc<Generator>, JoinHandle<()>)`. Дескриптор позволяет
/// остановить фоновую задачу: вызов `handle.abort()` отменяет её.
/// Дроп дескриптора **не** останавливает задачу — она продолжает работать до
/// явной отмены или завершения runtime.
///
/// **Аргументы**
/// - `$worker_id` — `u64` в диапазоне `0..=1023`; должен быть уникален среди всех
///   экземпляров `Generator`, работающих одновременно в вашем деплое.
/// - `$epoch` — [`SystemTime`](std::time::SystemTime), точка отсчёта для временных меток.
///
/// **Возвращает** `(Arc<`[`Generator`](crate::Generator)`>, JoinHandle<()>)`
///
/// # Паника
///
/// Паникует если `worker_id >= 1024`. Должен вызываться внутри async-контекста tokio.
#[macro_export]
macro_rules! tokio_run {
    ($worker_id:expr, $epoch:expr $(,)?) => {{
        let generator = ::std::sync::Arc::new($crate::Generator::new($worker_id, $epoch));

        generator.update_time();

        let updater = ::std::sync::Arc::clone(&generator);
        let handle = ::tokio::spawn(async move {
            let mut interval = ::tokio::time::interval(::std::time::Duration::from_millis(100));

            loop {
                interval.tick().await;
                updater.update_time();
            }
        });

        (generator, handle)
    }};
}

/// En: Generates a unique ID inside a tokio async context, yielding to the
/// executor on each retry.
///
/// On [`Error::StateUpdating`](crate::Error::StateUpdating) or
/// [`Error::Blocked`](crate::Error::Blocked) the macro calls
/// `tokio::task::yield_now().await`, allowing other tasks — including the
/// background `update_time` job — to make progress before retrying.
///
/// **Arguments**
/// - `$generator` — an expression that evaluates to `Arc<`[`Generator`](crate::Generator)`>`
///   or any type that derefs to `Generator`.
///
/// **Returns** [`BeakId`](crate::BeakId)
///
/// Must be used inside a `tokio` async context with `.await` support.
///
/// Ru: Генерирует уникальный ID внутри async-контекста tokio, уступая исполнителю
/// при каждой повторной попытке.
///
/// При [`Error::StateUpdating`](crate::Error::StateUpdating) или
/// [`Error::Blocked`](crate::Error::Blocked) макрос вызывает
/// `tokio::task::yield_now().await`, позволяя другим задачам — включая фоновую
/// задачу `update_time` — продвинуться перед повторной попыткой.
///
/// **Аргументы**
/// - `$generator` — выражение, результатом которого является `Arc<`[`Generator`](crate::Generator)`>`
///   или любой тип, разыменовываемый в `Generator`.
///
/// **Возвращает** [`BeakId`](crate::BeakId)
///
/// Должен использоваться внутри async-контекста `tokio` с поддержкой `.await`.
#[macro_export]
macro_rules! tokio_generate {
    ($generator:expr) => {{
        loop {
            match $generator.generate() {
                Ok(id) => break id,
                Err(_) => ::tokio::task::yield_now().await,
            }
        }
    }};
}
