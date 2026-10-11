# ORM / data-layer paired benchmark

The harness that judges the v1.4.0 data-layer work (see
`docs/releases/v1.4.0-plan.md`, "Where the time goes"). No speedup figure goes
into release notes unless it comes out of this harness.

| File | What it is |
|---|---|
| `app.py` | Cello app with one endpoint per row of the plan's table, plus a hot-path control |
| `seed.py` | Creates and seeds `bench_users` (200 rows × 8 columns) and the empty `bench_users_w`. `--drop` removes both |
| `paired.py` | Runs the paired A/B comparison with `wrk` (standard library only) |
| `profile_stages.py` | In-process timing of each `orm_list` stage (no HTTP) |
| `RESULTS.md` | A/A noise floor, v1.3.0 baseline numbers, profiling breakdown |

## Endpoints

| name | route | plan row |
|---|---|---|
| `control` | `GET /control` → `{"ok": true}` | hot-path control: should not move when only the data layer changes |
| `orm_list` | `GET /orm/list` | (a) 200 rows × 8 cols via ORM, `{"users": [u.to_dict() for u in users]}` |
| `orm_list_qs` | `GET /orm/list_qs` | (a') same rows, `return {"users": qs}` — the Phase 02 row → JSON fast path. A build without `QuerySet.json` serves the `to_dict()` shape, so A/B compares the best shape each build offers |
| `raw_list` | `GET /raw/list` | (b) same rows via `request.database.fetch` |
| `orm_get` | `GET /orm/get/42` | (c) `User.objects.get(id=...)` |
| `insert` | `POST /orm/insert` | (d) `UserW.objects.create(...)` into `bench_users_w` |
| `validate` | `POST /validate` | (f) `body=` validation against a 20-field DTO |

The 8 columns are int, text, bool, float, timestamptz, uuid, jsonb and numeric.
`uuid` and `numeric` use small `Field` subclasses defined in `app.py`, because
the ORM does not ship them.

To keep the table size stable, the read table is never written during a run.
Inserts go to `bench_users_w`, which the harness truncates (`POST
/_bench/reset`, outside the timed window) before every timed `insert` run. So
each A run and each B run starts from an empty table.

Five opt-in `prof_*` endpoints break `orm_list` down by stage:
`prof_orm_no_serialize`, `prof_fetch_only`, `prof_select1`, `prof_async_noop`
and `prof_pg_json`. They are not in the default set; pass them with
`--endpoints`. `RESULTS.md` explains how to read them.

## Running

Both builds must be installed in **separate venvs**. Never benchmark the
editable dev install.

```bash
# build a wheel for a given checkout (separate target dir so you don't disturb other builds)
CARGO_TARGET_DIR=/path/to/target maturin build --release -o /path/to/wheels -i python3.12
python3.12 -m venv /path/to/venvB && /path/to/venvB/bin/pip install /path/to/wheels/cello-*.whl

# compare: A = baseline, B = candidate
python3 benchmarks/orm/paired.py \
    --a /path/to/venvA/bin/python --b /path/to/venvB/bin/python \
    --json /tmp/paired.json
```

Useful flags: `--endpoints orm_list,raw_list` · `--rounds 10` · `--duration 5`
· `--warmup 5` · `--threads 2` · `--conns 32` · `--order abba|ab` ·
`--max-external 1.5` · `--dip 0.8` · `--max-retries 3` · `--dsn ...` (also
`BENCH_DSN`) · `--port-a 8811 --port-b 8812` · `--no-seed`. The defaults take
about 15–20 minutes for the six endpoints.

Postgres must be reachable. The default DSN is the one the test suite uses:
`postgresql://postgres:postgres@127.0.0.1:5432/cello_test`. The harness reseeds
the tables at start-up (`--no-seed` skips this). To clean up afterwards, run
`venv/bin/python benchmarks/orm/seed.py --drop`.

Ctrl-C or any error stops both servers (each runs in its own process group).
Server logs and the generated `wrk` Lua scripts go into `--logdir`, or a temp
dir if you don't set one.

## Why paired

This box is WSL2 on a shared CPU. Back-to-back runs of the same binary vary by
about ±20%, and that noise has invented "regressions" before. The harness
therefore:

1. **Runs both servers at the same time** for the whole session, each on its
   own port and from its own venv. Neither gets a warmer machine.
2. **Interleaves** the timed runs. By default each round flips which build
   goes first (A,B / B,A / …, an ABBA design). That cancels linear drift and
   any bias toward whichever build runs second. `--order ab` gives a strict
   A,B,A,B order.
3. **Compares means** across rounds. It also reports the median of the
   per-round paired ratios and their min/max, so you can see how far a single
   A-vs-B pair can wander.
4. **Re-runs contaminated rounds.** If another process on the box used more
   than `--max-external` cores during a run (measured from `/proc/stat`, minus
   the servers, wrk and postgres), that round is thrown away and repeated.
   After all rounds, any round where a run fell below `--dip` × *its own
   build's* median is also repeated. This catches the periodic ~30 s stalls on
   this box. The table's "rounds retried / flagged" column counts both kinds.
   Use `--dip 0` to switch the second check off when you suspect a build
   itself stalls intermittently.

Do not use a Python `requests`-style load client. It saturates at a few hundred
rps, so it measures the client and not the server. Use `wrk`.

Default load is `wrk -t2 -c32`, 10 rounds × 5 s, with 5 s of warmup per server
per endpoint. Each server is one process (`workers=1`). Two `wrk` threads
saturate one Cello process on this 8-core box and leave cores free for
Postgres. 32 connections keep the DB pool (16) busy: on `control`, -c8 gave
36k rps, -c32 38k and -c64 43k, but at -c64 p99 rose 10×. Shorter, more numerous
runs interleave A and B more finely than 5 × 10 s, so a burst of co-tenant load
lands on both builds. With these settings the A/A mean delta stayed within
±1.5% on every endpoint (see `RESULTS.md`).

## Reading the output

```
| endpoint | mean rps A | mean rps B | delta B vs A | A min–max | B min–max | paired-round delta range | p50 A / B | p99 A / B |
```

- **delta B vs A** is the headline number (mean B / mean A − 1).
- **paired-round delta range** is the smallest and largest single-round B/A
  ratio, which shows how much one pair can lie.
- **Noise floor.** `RESULTS.md` has an A/A run, where the same wheel is
  installed in both venvs. Treat a delta as real only if it is clearly larger
  than that endpoint's A/A delta *and* outside its A/A paired-round range.
  Re-run once before you quote it. If the result does not reproduce, it is
  noise.
- **`control` is the canary.** A data-layer change should leave it flat
  (within its A/A floor). If `control` moves by the same amount as everything
  else, you are looking at machine drift, not your change.
- Any `NON-2xx` line invalidates that endpoint's numbers. The harness prints a
  warning when this happens.
- p50 and p99 are the median across runs of `wrk --latency` percentiles.
  Compare them only between A and B from the same session.
