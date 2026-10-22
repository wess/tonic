#[path = "support/process.rs"]
mod process;

use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn native_resource_handles_survive_process_exit_ets_and_moving_gc() {
    let dir = std::env::temp_dir().join(format!("tonicresources{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("resources.exs");
    fs::write(
        &script,
        r#"
parent = self()
table = :ets.new(:resources, [:public])
{pid, monitor} = spawn_monitor(fn ->
  atomics = :atomics.new(2, [])
  :atomics.put(atomics, 1, 41)
  counters = :counters.new(2, [:write_concurrency])
  :counters.add(counters, 2, 100)
  :ets.insert(table, {:handles, atomics, counters})
  send(parent, {:handles, atomics, counters})
end)
{atomics, counters} = receive do
  {:handles, a, c} -> {a, c}
after
  5000 -> raise "handles timeout"
end
receive do
  {:DOWN, ^monitor, :process, ^pid, :normal} -> :ok
after
  5000 -> raise "exit timeout"
end
for _ <- 1..12, do: :erlang.garbage_collect()
roundtrip = :erlang.binary_to_term(:erlang.term_to_binary(atomics))
true = roundtrip == atomics
41 = :atomics.get(roundtrip, 1)
42 = :atomics.add_get(atomics, 1, 1)
:counters.add(counters, 2, 23)
[{:handles, stored_a, stored_c}] = :ets.lookup(table, :handles)
true = stored_a == atomics
true = stored_c == counters
:ets.delete(table)
worker = spawn(fn ->
  receive do
    {:handles, a, c} ->
      for _ <- 1..12, do: :erlang.garbage_collect()
      send(parent, {:values, :atomics.get(a, 1), :counters.get(c, 2)})
  end
end)
send(worker, {:handles, atomics, counters})
receive do
  {:values, a, c} -> IO.inspect({a, c})
after
  5000 -> raise "values timeout"
end
stream = :zlib.open()
:ok = :zlib.deflateInit(stream)
stream = :erlang.binary_to_term(:erlang.term_to_binary(stream))
worker = spawn(fn ->
  receive do
    {:stream, z} ->
      for _ <- 1..12, do: :erlang.garbage_collect()
      compressed = :zlib.deflate(z, "portable", :finish)
      :ok = :zlib.deflateEnd(z)
      :ok = :zlib.close(z)
      send(parent, {:inflated, :zlib.uncompress(IO.iodata_to_binary(compressed))})
  end
end)
:ok = :zlib.set_controlling_process(stream, worker)
send(worker, {:stream, stream})
receive do
  {:inflated, text} -> IO.puts(text)
after
  5000 -> raise "zlib timeout"
end
for _ <- 1..30 do
  {pid, ref} = spawn_monitor(fn ->
    :atomics.new(1024, [])
    :counters.new(1024, [])
    z = :zlib.open()
    :ok = :zlib.deflateInit(z)
  end)
  receive do
    {:DOWN, ^ref, :process, ^pid, :normal} -> :ok
  after
    5000 -> raise "cleanup timeout"
  end
end
IO.puts("cleaned")
"#,
    )
    .unwrap();
    let mut beam = Command::new("elixir");
    beam.arg(&script);
    let beam = process::execute(beam, Duration::from_secs(120));
    assert!(
        beam.status.success(),
        "{}",
        String::from_utf8_lossy(&beam.stderr)
    );
    assert_eq!(beam.stdout, b"{42, 123}\nportable\ncleaned\n");
    let binary = dir.join("resources");
    let mut build = Command::new(env!("CARGO_BIN_EXE_tonic"));
    build.args([
        "build",
        script.to_str().unwrap(),
        "-o",
        binary.to_str().unwrap(),
    ]);
    let build = process::execute(build, Duration::from_secs(240));
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    for stress in ["0", "1"] {
        let mut native = Command::new(&binary);
        native
            .env("TONIC_GC_STRESS", stress)
            .env("TONIC_GC_VERIFY", "1");
        let native = process::execute(native, Duration::from_secs(120));
        assert!(
            native.status.success(),
            "stress={stress}: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(native.stdout, beam.stdout, "stress={stress}");
    }
    fs::remove_dir_all(dir).unwrap();
}
