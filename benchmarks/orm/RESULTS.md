# ORM benchmark results

## Run metadata

- **Date:** 2026-10-10
- **Commit:** `f9119e0` ("perf issues"), the v1.3.0 baseline. Built from a clean
  detached worktree, not from the dirty working tree.
- **Wheel:** `cello_framework-1.3.0-cp312-abi3-manylinux_2_34_aarch64.whl`,
  sha256 `2f92d002…1abc1003`, `maturin build --release` with the repo's release
  profile (fat LTO, stripped).
- **Machine:** WSL2 (kernel 6.18.33.1-microsoft-standard-WSL2), Snapdragon X
  (Oryon), **aarch64**, 8 vCPU, 7.7 GB RAM. Python 3.12.3. PostgreSQL 16.15 on
  127.0.0.1:5432 (same box). wrk 4.1.0.
- **Co-tenancy:** other agents were compiling Cello and running test suites
  during this session. Load average swung between about 2 and 12. Some runs were
  thrown away and repeated because of it; see "Noise on this box" below.
- **Harness settings** (the defaults): `wrk -t2 -c32`, 10 rounds × 5 s, ABBA
  order, 5 s warmup per server per endpoint, one server process each,
  Postgres pool 16.

## A/A noise floor

The same wheel is installed in two venvs, and both servers run at the same
time.

| endpoint | mean rps A | mean rps B | delta (means) | median paired delta | single-round delta range | p50 A / B (ms) | p99 A / B (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| control | 46,523 | 46,171 | −0.8% | −2.9% | −10.5% … +16.3% | 0.63 / 0.70 | 1.25 / 1.19 |
| orm_list | 205 | 204 | −0.2% | +0.1% | −2.9% … +3.0% | 149 / 150 | 230 / 235 |
| raw_list | 266 | 267 | +0.4% | +0.5% | −1.5% … +2.0% | 118 / 117 | 168 / 168 |
| orm_get | 1,340 | 1,319 | −1.5% | −1.0% | −8.6% … +6.6% | 22.3 / 22.8 | 43.3 / 45.4 |
| insert | 1,368 | 1,375 | +0.5% | +0.4% | −7.6% … +11.3% | 21.3 / 21.1 | 40.7 / 40.0 |
| validate | 17,208 | 17,266 | +0.3% | +1.1% | −5.0% … +3.4% | 1.89 / 1.87 | 2.71 / 2.75 |

**How to use this.** With the default settings, the noise floor on the mean
delta is about **±1.5%** for every endpoint, and about ±3% on the `control`
median. Single rounds can swing 10–16%, so never read anything from one round.
Count a candidate delta as real only if it is **above about 3%** (or 5% for
`control`), the same direction shows in the median paired delta, and it
reproduces on a second run.

### Noise on this box (why the harness has retry logic)

The first three A/A attempts used plain mean-of-rounds and gave these mean
deltas between *identical* builds:

| attempt | control | orm_list | raw_list | orm_get | insert | validate |
|---|---:|---:|---:|---:|---:|---:|
| 5×10 s, co-tenant DB tests running | +4.1% | −12.9% | +6.9% | **+39.8%** | −6.0% | +6.8% |
| 5×10 s, co-tenant release build | +1.9% | +14.5% | −8.7% | −0.9% | −6.6% | **−20.1%** |
| 10×5 s + co-tenant CPU re-run only | +9.0% | −2.9% | +4.2% | −4.3% | +0.9% | −3.7% |
| **10×5 s + co-tenant re-run + dip re-run (final)** | −0.8% | −0.2% | +0.4% | −1.5% | +0.5% | +0.3% |

Two different things cause the noise, and the harness handles each one
separately:

1. **Co-tenant CPU.** Other builds and tests can take up to 7 cores. For each
   run, `paired.py` measures machine-wide busy CPU from `/proc/stat` and
   subtracts the benchmark's own CPU (both servers, wrk, all postgres
   processes). If either run in a round saw more than 1.5 other cores busy, the
   round is thrown away and repeated.
2. **Periodic dips.** Roughly every 30 s, one run loses 35–60% of its
   throughput while no co-tenant CPU is visible. In A/A runs this hits A and B
   equally. A CPU-spin probe caught it once as a machine-wide slowdown. A
   second probe and the TCP counters (listen drops, SYN retransmits) showed
   nothing, so the cause is not known. After all rounds, any round in which a
   run fell below 80% of *its own build's* median is re-run.

   Because each build is compared only with itself, a uniform A/B difference
   cannot trigger this. A build that dips *intermittently* would be masked,
   though. Check the "rounds retried" column, or run with `--dip 0` to see the
   raw numbers.

## v1.3.0 baseline (f9119e0)

Absolute rps for each endpoint, taken from the A side of the final A/A run,
with the B side for cross-reference. The Phase 02 claim must be judged against
these numbers, and always **paired against this wheel**, never against the
numbers alone.

| endpoint | plan row | rps (A / B) | p50 | p99 |
|---|---|---:|---:|---:|
| `control` | hot-path control | 46,523 / 46,171 | 0.63 ms | 1.25 ms |
| `orm_list` | (a) 200 rows × 8 cols, ORM + `to_dict()` | 205 / 204 | 149 ms | 230 ms |
| `raw_list` | (b) same rows, raw `db.fetch` | 266 / 267 | 118 ms | 168 ms |
| `orm_get` | (c) `.get(id=…)` | 1,340 / 1,319 | 22.3 ms | 43.3 ms |
| `insert` | (d) ORM insert, one row | 1,368 / 1,375 | 21.3 ms | 40.7 ms |
| `validate` | (f) `body=` 20-field DTO | 17,208 / 17,266 | 1.89 ms | 2.71 ms |

Latencies are at 32 connections against a single saturated process, so they
are mostly queueing. Compare them between A and B; do not read them as
absolute numbers.

## Profiling: where `orm_list` actually spends its time

There were three independent methods, and they agree.

### 1. Stage removal under load (server, paired A/A, 6 rounds)

Each `prof_*` endpoint removes one stage of `orm_list`. The server process is
saturated and the work is serialized on the GIL, so 1/rps is roughly the
service time per request.

| endpoint | what it does | rps (A / B) | ms/request |
|---|---|---:|---:|
| `orm_list` | full path | 206 / 206 | 4.85 |
| `prof_orm_no_serialize` | ORM + `to_dict()`, returns `{"n": 200}` | 389 / 394 | 2.57 |
| `raw_list` | `db.fetch`, returns rows (serialized) | 263 / 265 | 3.80 |
| `prof_fetch_only` | `db.fetch`, returns `{"n": 200}` | 675 / 667 | 1.48 |
| `prof_select1` | async handler + `fetchval("SELECT 1")` | 1,552 / 1,543 | 0.64 |
| `prof_async_noop` | async handler, no I/O | 2,523 / 2,541 | 0.40 |
| `control` | sync handler, `{"ok": true}` | ~46,000 | 0.02 |
| `prof_pg_json` | Postgres builds the JSON, Python passes one string through | 389 / 394 | 2.57 |

Derived breakdown of `orm_list` (4.85 ms):

| stage | ms | share | how it was derived |
|---|---:|---:|---|
| **Serialize the 200 dicts to JSON (Rust `write_json_value`)** | **2.28** | **47%** | orm_list − orm_no_serialize. raw_list − fetch_only gives 2.32, which agrees |
| `_from_row` + `to_dict()` + QuerySet | 1.09 | 22% | orm_no_serialize − fetch_only |
| 200 rows over the wire + `row_to_pydict` | 0.84 | 17% | fetch_only − select1 |
| one DB call (pool, bridge, round trip) | 0.24 | 5% | select1 − async_noop |
| async handler floor (coroutine dispatch on the asyncio loop) | 0.40 | 8% | async_noop |

`prof_pg_json` is **not** a usable ceiling. Making Postgres build 45 KB of JSON
with `json_agg` is about as slow as the Python path. It is kept only as a data
point.

### 2. In-process stage timing (`profile_stages.py`, no HTTP, median of 400)

| stage | µs | share |
|---|---:|---:|
| `db.execute` (Postgres + wire + bridge, rows discarded in Rust) | 2,531–2,708 | 35% |
| `row_to_pydict` + `pg_value_to_py` (fetch − execute) | 107–343 | 1.5–4.5% |
| `_from_row` × 200 | 580 | 8% |
| `to_dict()` × 200 | 575–578 | 8% |
| serialize (`Response.json` proxy) | 3,438–3,440 | 45–47% |
| QuerySet SQL assembly | 36 | 0.5% |

Reference points: the same payload through stdlib `json.dumps` takes **595 µs**,
about **5.8× faster** than Cello's Rust serializer on the same Python objects.
Here `db.execute("SELECT 1")` costs about 2.8–3.2 ms of latency, but a tight
loop of the same call measured about 0.2 ms. The bridge's wake-up latency
depends on context, and that 35% is latency, not CPU. In the loaded server the
DB call adds only about 0.24 ms (table 1).

### 3. Sampling profile (py-spy 0.4.2 `--native`, symbolized build of f9119e0)

These are on-CPU samples only. Waits and unresolved frames were dropped. The
sample is small (255 on-CPU samples) and blocking-mode sampling distorts it,
so treat it as directional only:

| category | share of on-CPU |
|---|---:|
| `write_json_value` (serializer), including failed-extract `PyErr` allocation | 36.5% |
| ORM `to_dict` | 14.5% |
| asyncio loop / bridge | 13.3% |
| socket / hyper | 9.8% |
| ORM `_from_row` | 7.1% |
| `row_to_pydict` / `pg_value_to_py` | 2.4% |
| other | 16% |

### Verdict on the "four materializations" thesis

The thesis is **partly confirmed, and the main cost is somewhere other than
the plan says**.

- **Confirmed:** the Python-side materializations are real but modest.
  `_from_row` + `to_dict()` cost about 1.1 ms, about 22% of `orm_list`.
  Removing them is worth roughly 1.3× on their own.
- **Refuted:** `row_to_pydict` is *not* a significant stage (about 2–5%). The
  plan's table books 1,600 Python objects against it. They exist, but creating
  them in Rust is cheap.
- **Missed:** the stage the plan marks as free ("SIMD JSON walks the dicts —
  Rust —") is the **largest single cost, about 47%**. The serializer is
  neither SIMD nor fast on Python objects. `write_json_value` (`src/json.rs`)
  tries `extract::<bool>`, then `i64`, `u64`, `f64`, before `String`. Every
  string, dict and list value therefore raises and discards up to three Python
  exceptions before it hits the right branch (`PyErr` allocations show up in
  the native profile). Stdlib `json.dumps` serializes the same payload about
  5.8× faster.
- **Implication for Phase 02:** the planned fast path, from pg wire straight
  to JSON bytes, skips this serializer too, so the *remedy* still targets the
  right endpoint. Ideal upper bound is ≈ 4.85 → ~1.5 ms/request, about 3×.
  That is an estimate, not a claim. But most of that win comes from bypassing
  `write_json_value`, not from skipping `_from_row`/`to_dict`.

  A much cheaper, independent fix is to check exact types first (`PyString`,
  `PyDict`, `PyList`, `PyBool`, `PyLong`, `PyFloat`) before any `extract`. That
  would speed up *every* endpoint that returns a dict, and on its own it could
  recover a large share of the `orm_list` gain. If that lands before
  Phase 02, the Phase 02 gain will shrink accordingly. Measure both, paired.
- **`.get` / insert:** the plan says "DB round trip dominates", but the async
  handler floor does. An async handler with no I/O tops out at about 2,500 rps
  (0.40 ms) against about 46,000 for a sync handler. That 0.40 ms is about 53%
  of `orm_get`'s 0.75 ms, and one DB call adds only about 0.24 ms. Work on the
  async bridge, not the ORM, is what moves these rows.

## Correctness findings (baseline, not fixed — out of scope)

- **`numeric` columns decode to the string
  `"<unsupported column type: numeric>"`** in `db.fetch` and the ORM
  (`pg_value_to_py` in `src/db/value.rs` has no NUMERIC arm, and the `String`
  fallback rejects numeric). This is silent data loss. The `balance` column in
  every baseline response carries this placeholder. A fixed build will
  serialize real numbers, so expect a small payload-size difference that is
  unrelated to performance.
- **ORM inserts cannot bind `timestamptz`, `uuid` or `numeric`** values
  (`SqlParam::Text` is sent as binary text). For that reason `bench_users_w`
  leaves those columns nullable, and the insert endpoint sends NULL for them.
- A standalone server once ignored both SIGINT and SIGTERM and needed SIGKILL.
  py-spy had been attached to it earlier, so this is unconfirmed. `paired.py`
  escalates to SIGKILL after 3 s.

## Current working tree vs baseline

**Not run.** Applying the working tree's uncommitted changes onto a scratchpad
worktree (with `sync_from_main.sh`) was denied by the session's permission
classifier. To run it once that is allowed, see the README ("Running") and the
command in the final report.
