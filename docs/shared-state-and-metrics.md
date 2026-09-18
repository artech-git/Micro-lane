# Shared state & metrics: design notes

Design record for the refactor of `Metrics`, `UpstreamNameServer`, and the per-packet
shared-state plumbing. Written so the *reasoning* survives, not just the diff — several of
these decisions look arbitrary until you know which alternative was measured and rejected.

## Context

Every received UDP packet passed through a receive loop that cloned four separate `Arc`s,
plus one more inside `ClientGuard::check_rate`, plus one per upstream lookup inside
`UpstreamNameServer::lookup`. `Metrics` held nine independent atomics, two of which were
touched on every query.

Counted per query, that was roughly **12 atomic read-modify-writes on refcounts versus 2 on
the actual instrumentation** — the bookkeeping cost about six times what the thing being
booked cost. Refcount traffic is not free: each clone/drop is a `lock`-prefixed RMW on a
cache line shared by every worker thread, and `Arc::drop` additionally needs an acquire
fence on the zero check.

## Decisions

### 1. Counters became `[AtomicU64; N]` indexed by an enum

`Metrics` now holds one array addressed by `Counter`, replacing nine named fields and
roughly twenty hand-written accessor methods with `incr(Counter)` / `get(Counter)`.

**Why:** adding a counter was a three-method change (field, recorder, reader) across two
files. It is now one enum variant. Call sites pass a literal, so the index constant-folds
after inlining and the bounds check disappears — the generated code is the same as the
named-field version.

This is a *representation* change, not a compression change. Each counter is still a full
`u64`.

### 2. `queries_total` is derived, not stored

```rust
queries_total() == get(QueriesOk) + get(QueriesServfail) + get(QueriesFormerr)
```

**Why:** `handle_query` incremented `queries_total` and then exactly one of the three
outcome counters, on every path. The total carried no information the other three didn't
already have. Dropping it takes the hot path from **two atomic RMWs per query to one**, with
zero information loss.

The three loads are not a consistent snapshot. That is acceptable and deliberate: these are
monotonic counters, and every metrics consumer already tolerates that skew. A brief
under-count between two increments is not a correctness problem for a counter that only ever
goes up.

### 3. `set_circuit_open` only writes on a transition

**Why:** `lookup` reports "breaker closed" after *every* successful upstream response, and a
recursive resolution issues several lookups per client query. The old unconditional `store`
dirtied that cache line — invalidating it in every other core — each time, to write a value
that was already `false` in virtually every case. A load is a cheap shared read; the store is
the expensive part. So we load first and store only when the value actually changes.

### 4. The circuit breaker is stored by value, not behind an `Arc`

```rust
// failsafe-1.3.0/src/state_machine.rs:53
pub struct StateMachine<POLICY, INSTRUMENT> { inner: Arc<Inner<POLICY, INSTRUMENT>> }
```

**Why:** `failsafe::StateMachine` is *already* a handle wrapping an `Arc<Inner>` — it
measures 8 bytes and implements `Clone`. The field was therefore `Arc<Arc<Inner>>`. The
comment claiming it was "shared across clones" described a `Clone` impl that
`UpstreamNameServer` never had; the breaker is shared because the *resolver* is reached
through an `Arc`, which was already true.

Removing the outer `Arc` costs nothing and removes one dependent load from the chain
(`Arc<AppState>` → `Arc<Breaker>` → `Inner` becomes `Arc<AppState>` → `Inner`) plus one
startup allocation.

### 5. `Metrics` is a *parameter* to `lookup`, not a *field* of the resolver

This distinction is the load-bearing one, and it is easy to get wrong in the other direction.

A `&Metrics` **field** would force `UpstreamNameServer<'a>`, and that fails for four reasons:

1. **The lifetime goes viral.** Every signature naming the type grows a parameter.
2. **`tokio::spawn` requires `'static`.** A future capturing `UpstreamNameServer<'a>` is only
   valid for `'a`. You get `argument requires that '1 must outlive 'static`, and there is no
   borrow-checker trick around it.
3. **`AppState` would become self-referential.** A struct owning `Metrics` *and* a resolver
   borrowing that field cannot be expressed in safe Rust. This is where the approach dies.
4. **Construction order gets rigid.** `Metrics` could never move after the resolver saw it.

A `&Metrics` **parameter** has none of those problems. The borrow only has to outlive the
`await` of `lookup`, which sits inside the caller's frame where the owner is already alive.
It is `Send` across `.await` because `Metrics` is `Sync`.

**The rule this encodes:** `Arc` belongs at the `tokio::spawn` boundary, where `'static` is
genuinely required. Everywhere inside that boundary, use a plain `&`.

### 6. One `Arc<AppState>` instead of four separate `Arc`s

`AppState` bundles socket, resolver, metrics, and client guard. The receive loop clones it
once per packet.

**Why:** the spawn boundary needs *an* owned handle — that much is unavoidable. It does not
need four. One `Arc::clone` per packet touches one refcount cache line instead of four, and
everything inside the task is then reached by plain `&`. `Arc<UdpSocket>` disappeared
entirely: the socket is owned by `AppState`, so the outer `Arc` already shares it.

### 7. `check_rate` uses a read lock on the steady-state path

`DashMap::entry()` always takes the shard's **write** lock. With 64 shards (the default is
`available_parallelism() * 4`, rounded up), every packet — admitted or rejected — serialised
all clients hashing to the same shard.

The fast path now tries `get()` (read lock) and falls back to `entry()` only for a
first-seen address. This works because `ClientEntry` is entirely interior mutability
(`Mutex<TokenBucket>`, `AtomicI64`), so a shared reference suffices for both `touch()` and
`try_consume()`.

Second benefit: the `Arc` clone moved *after* the rate decision, so a rejected packet now
costs no clone/drop pair at all. Previously a rate-limited flood paid nearly the same
bookkeeping as an admitted one.

## Alternatives considered and rejected

### Bit-packing the counters into a single integer

The original question was whether the atomics could be losslessly packed into one integer.
They cannot, and it would be slower if they could:

- Eight counters in one `u64` is 8 bits each — overflow at 255. Keeping `u32` range fits
  exactly two counters per word. `AtomicU128` is not stable. **Lossless and single-integer
  are mutually exclusive** for counters that must be unbounded.
- A separate atomic increments with one `lock xadd` — wait-free, no retry. A *packed* field
  needs a masked CAS loop, because a plain shifted `fetch_add` silently carries into the
  neighbouring counter on overflow (that is precisely the lossy part). CAS loops retry under
  contention.
- Three distinct words let threads increment different outcomes in parallel. One packed word
  forces every thread to serialise on it — trading incidental false sharing for mandatory
  true sharing.

The real win was available at the arithmetic level instead (decision 2), not the bit level.

### `&'static Metrics` via `Box::leak` or a `OnceLock`

This compiles, satisfies `'static`, and reaches literally zero refcount traffic — `&'static T`
is `Copy`. Rejected because:

- A `OnceLock` global means every test shares one `Metrics`. Counters bleed between tests and
  `cargo test`'s thread-parallel runner makes any count assertion flaky. `Box::leak` per test
  avoids sharing but leaks per test.
- It permits only one resolver instance per process — no integration test standing up two.

Not worth roughly two atomics per query once decision 6 had already taken the per-packet
count from four refcounts to one.

## Measured

On a 12-core machine: `ClientEntry` = 72 bytes, `StateMachine` = 8 bytes (a pointer),
DashMap default = 64 shards.

Per packet, in the hot path:

| | Before | After |
|---|---|---|
| `Arc` clone/drop pairs | 4, plus 1 per upstream lookup | 1 |
| Metrics atomic RMWs | 2 | 1 |
| DashMap lock kind | write | read (steady state) |
| Refcount cost on rejection | clone + drop | none |

Verified against a live instance: three resolutions plus one malformed packet produced
`{"total":4,"ok":3,"servfail":0,"formerr":1}` — the derived total is exact. Twenty concurrent
queries against a capacity-5 bucket produced 5 admitted and 15 rate-limited.

## Known open issue

The token bucket bounds each client's *rate*, and therefore bounds the `Arc<ClientEntry>`
refcount indirectly via Little's Law — at the defaults, `capacity + refill × timeout` =
`100 + 10×5` ≈ 150 in-flight per client.

It bounds neither of the things that actually matter:

- **The global `Arc<AppState>` refcount.** The bucket is per-IP; `AppState` is shared by all
  clients. N distinct source IPs at 10/s each is unbounded in N, and nothing caps N.
- **Memory.** An entry is created on an address's *first* packet, before any rate decision.
  At roughly 190 bytes and three allocations per distinct source IP, and with UDP source
  addresses trivially spoofable, the `DashMap` grows until the sweep reclaims it —
  `idle_ttl_secs` defaults to 300.

That is the real exhaustion surface and it is orthogonal to refcounting. Candidate fixes: a
hard cap on `entries.len()` with rejection or LRU eviction past it, a shorter idle TTL, or
keying on /24 (v4) and /64 (v6) — the last also closes per-IP-rotation evasion within a
subnet.

Minor related note: sweeping an entry discards its breaker state, so a client that tripped
its breaker gets a fresh one after going idle past the TTL. Self-limiting, since it must go
quiet to earn it, but it is the current behaviour.
