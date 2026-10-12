# helloweb

A small JSON service built with Plug, Bandit, and Ecto on SQLite. On startup
it creates the `greetings` table and serves:

| Method | Path | Result |
| --- | --- | --- |
| GET | `/` | `Hello, web!` |
| GET | `/greetings` | all greetings |
| GET | `/greetings/:id` | one greeting, or 404 |
| POST | `/greetings` | creates a greeting from `{"name": "..."}`; returns 422 if invalid |

## Run on the BEAM

```sh
mix deps.get
mix run --no-halt
```

## Build a native executable with Tonic

```sh
mix deps.get
tonic build . -o helloweb
./helloweb
```

The executable contains no BEAM bytecode. Tonic compiles every dependency from
source, including `telemetry` (a rebar3 package) and the C NIF in `exqlite`,
which is linked into the binary.

```sh
curl localhost:4000/
curl -X POST -H 'content-type: application/json' -d '{"name":"world"}' localhost:4000/greetings
curl localhost:4000/greetings
```

`config/runtime.exs` reads `PORT` (default `4000`) and `DATABASE` (default
`helloweb.db`) when the program starts, on both the BEAM and the native build.
