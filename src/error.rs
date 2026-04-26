/// En: Errors returned by [`Generator`](crate::Generator) and [`BeakId`](crate::BeakId).
///
/// Ru: Ошибки, возвращаемые [`Generator`](crate::Generator) и [`BeakId`](crate::BeakId).
#[derive(Debug)]
pub enum Error {
    /// En: The background job is currently updating the timestamp.
    /// This state lasts nanoseconds; retry immediately.
    ///
    /// Ru: Фоновая задача выполняет обновление timestamp.
    /// Это состояние длится наносекунды; повторите вызов немедленно.
    StateUpdating,

    /// En: All 10 virtual time windows are exhausted. The generator will
    /// unblock automatically on the next [`update_time`](crate::Generator::update_time)
    /// call once real time has advanced far enough.
    ///
    /// Ru: Исчерпаны все 10 виртуальных временных окон. Генератор разблокируется
    /// автоматически при следующем вызове [`update_time`](crate::Generator::update_time)
    /// как только реальное время продвинется достаточно далеко.
    Blocked,

    /// En: `worker_id` passed to [`Generator::try_new`](crate::Generator::try_new)
    /// is out of range. Valid values are `0..=1023`.
    ///
    /// Ru: `worker_id`, переданный в [`Generator::try_new`](crate::Generator::try_new),
    /// вне допустимого диапазона. Допустимые значения: `0..=1023`.
    InvalidWorkerId,

    /// En: The string passed to [`BeakId::from_base62`](crate::BeakId::from_base62)
    /// is not a valid 11-character base62 value.
    ///
    /// Ru: Строка, переданная в [`BeakId::from_base62`](crate::BeakId::from_base62),
    /// не является корректным 11-символьным значением base62.
    InvalidBase62,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::StateUpdating => f.write_str("generator is currently updating its timestamp"),
            Error::Blocked => f.write_str("generator is blocked: virtual windows exhausted"),
            Error::InvalidWorkerId => f.write_str("worker_id must be less than 1024"),
            Error::InvalidBase62 => f.write_str("invalid base62 string"),
        }
    }
}

impl std::error::Error for Error {}
