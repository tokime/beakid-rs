# beakid

En: Lock-free unique ID generator inspired by Twitter Snowflake. Designed for high-throughput async services where allocating a UUID per request is too slow.

Ru: Генератор уникальных ID без блокировок, вдохновлённый Twitter Snowflake. Создан для высоконагруженных async-сервисов, где аллоцировать UUID на каждый запрос слишком дорого.

---

## ID layout

| Bits    | Field       | Size    | Description                                      |
|---------|-------------|---------|--------------------------------------------------|
| [63..29] | timestamp  | 35 bits | 100 ms units since epoch (~109 years range)      |
| [28..10] | sequence   | 19 bits | up to 524 288 IDs per window                     |
| [9..0]   | worker_id  | 10 bits | 0..=1023, distinguishes generator instances      |

En: IDs are monotonically increasing within a single `Generator` instance and sortable by creation time across the cluster within the same 100 ms window.

Ru: ID монотонно возрастают в рамках одного экземпляра `Generator` и сортируемы по времени создания в кластере в рамках одного 100 мс окна.

---

## How it works

En: The generator is created with a unique `worker_id` and is designed to be shared across threads within a single worker via `Arc`. ID generation reduces to a single `fetch_add` on an `AtomicU64` — no mutex, no system call.

For the generator to function, a background worker must be running and calling `update_time` every 100 ms.

Each 100 ms window holds up to 524 288 IDs, giving a sustained throughput of ~5.2 M IDs/s. Virtual time windows allow the generator to absorb burst traffic by borrowing time from the future. The virtual clock can run at most 1 second ahead of real time. If it goes too far, the generator blocks — preventing the virtual clock from drifting indefinitely.

Ru: Генератор создаётся с уникальным `worker_id` и рассчитан на один воркер с расшариванием между потоками через `Arc`. Генерация ID сводится к одному `fetch_add` на `AtomicU64` — без мьютексов, без системных вызовов.

Для работы генератора обязательно должен быть запущен отдельный воркер, вызывающий `update_time` каждые 100 мс.

Каждое 100 мс окно вмещает до 524 288 ID, что даёт устойчивую пропускную способность ~5.2 млн ID/с. Виртуальные временные окна позволяют пережить всплески активности, занимая время в будущем. Виртуальные часы могут уйти максимум на 1 секунду вперёд реального времени. Если они уходят слишком далеко — генератор блокируется, не позволяя виртуальному времени убегать бесконечно.

---

## Usage

En: In case of blocking, `generate()` returns `Error::Blocked`. You can handle it yourself, or use the macro that does it automatically:

Ru: В случае блокировки `generate()` вернёт `Error::Blocked`. Вы можете обработать ошибку самостоятельно, либо использовать макрос, который делает это автоматически:

```rust
let id = tokio_generate!(generator);
// or / или
let id = smol_generate!(generator);
```

En: You can also use `must_generate()`, but for async runtimes their own cooperative yield mechanisms are recommended.

Ru: Так же можно использовать `must_generate()`, но для асинхронных рантаймов рекомендуется использовать их собственные механизмы передачи управления.

---

En: For convenient setup use the runtime macros — they create the generator and start the background `update_time` worker automatically:

Ru: Для удобного создания используйте макросы рантайма — они создают генератор и автоматически запускают фоновый воркер `update_time`:

**Tokio**

```rust
let (generator, handle) = beakid::tokio_run!(worker_id, SystemTime::UNIX_EPOCH);
// En: Dropping `handle` does NOT stop the task — call handle.abort() to stop it.
// Ru: Дроп handle не останавливает задачу — вызовите handle.abort() чтобы остановить её.
```

**Smol**

```rust
let (generator, handle) = beakid::smol_run!(worker_id, SystemTime::UNIX_EPOCH);
// En: Dropping `handle` cancels the task immediately — keep it alive for the generator's lifetime.
// Ru: Дроп handle отменяет задачу немедленно — держите его живым всё время работы генератора.
```

---

## Base62

En: Every `BeakId` can be encoded as a fixed 11-character base62 string (`0–9`, `A–Z`, `a–z`) for use in URLs, database keys, or logs.

Ru: Каждый `BeakId` можно закодировать в фиксированную 11-символьную строку base62 (`0–9`, `A–Z`, `a–z`) для использования в URL, ключах БД или логах.

```rust
let s = id.base62();
let id2 = BeakId::from_base62(&s)?;
assert_eq!(id, id2);
```

---

## Known limitations

En:
- **worker_id must be unique across the deployment.** Two generators with the same `worker_id` running simultaneously will produce duplicate IDs. Coordination (e.g. via Redis, etcd, or static config) is the caller's responsibility.
- **Clock skew.** If the system clock jumps backward, IDs generated after the jump can collide with or sort before existing IDs. The generator does not detect backward clock jumps.
- **Cache line contention under many threads.** All threads increment the same `AtomicU64`. On NUMA systems or with many cores, MESI cache coherence traffic can reduce throughput. Benchmarks show tokio scaling poorly past 8 threads on a shared generator; smol scales better due to simpler scheduling.

Ru:
- **worker_id должен быть уникален в рамках всего деплоя.** Два генератора с одинаковым `worker_id`, работающие одновременно, будут создавать дубликаты. Координация (например, через Redis, etcd или статичный конфиг) — ответственность вызывающего.
- **Drift часов.** Если системные часы прыгают назад, ID, сгенерированные после прыжка, могут совпасть с существующими или нарушить порядок сортировки. Генератор не обнаруживает прыжки назад.
- **Конкуренция за кэш-линию при большом числе потоков.** Все потоки инкрементируют один `AtomicU64`. На NUMA-системах или при большом числе ядер трафик когерентности кэша (MESI) снижает пропускную способность. Бенчмарки показывают, что tokio плохо масштабируется свыше 8 потоков на общем генераторе; smol масштабируется лучше из-за более простого планировщика.

---

## License

MIT
