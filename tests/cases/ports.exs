defmodule PortCase do
  # Messages that arrive within a short quiet period, with ports and pids
  # replaced by stable names so the output does not depend on numbering.
  def flush(names, acc \\ []) do
    receive do
      m -> flush(names, [m | acc])
    after
      300 -> acc |> Enum.reverse() |> Enum.map(&name(&1, names))
    end
  end

  def name(t, names) when is_port(t) or is_pid(t) or is_reference(t), do: Map.get(names, t, :other)
  def name(t, names) when is_tuple(t), do: t |> Tuple.to_list() |> Enum.map(&name(&1, names)) |> List.to_tuple()
  def name(t, names) when is_list(t), do: Enum.map(t, &name(&1, names))
  def name(t, _), do: t

  # All output until the exit status, joined, so pipe chunking cannot show.
  def collect(port, acc \\ "") do
    receive do
      {^port, {:data, data}} -> collect(port, acc <> IO.iodata_to_binary(data))
      {^port, {:exit_status, status}} -> {acc, status}
    after
      5000 -> {:timeout, acc}
    end
  end

  def echo(port, data) do
    Port.command(port, data)

    receive do
      {^port, {:data, reply}} -> reply
    after
      5000 -> :timeout
    end
  end

  def show(label, value), do: IO.puts("#{label}: #{inspect(value)}")

  def error(fun) do
    fun.()
  rescue
    e -> {e.__struct__, Exception.message(e)}
  end
end

alias PortCase, as: P
Process.flag(:trap_exit, true)
me = self()

# --- the port term ---
p = Port.open({:spawn, "cat"}, [:binary])
q = Port.open({:spawn, "cat"}, [:binary])
names = %{p => :p, q => :q, me => :me}
P.show("is_port", {is_port(p), is_reference(p), is_pid(p), is_port(make_ref())})
P.show("inspect", inspect(p) =~ ~r/^#Port<0\.\d+>$/)
P.show("port_to_list", :erlang.port_to_list(p) == String.to_charlist(inspect(p)))
P.show("order", Enum.sort([me, q, make_ref(), fn -> 1 end, 1, :a, p]) |> Enum.map(&P.name(&1, Map.put(names, 1, 1))) |> Enum.map(fn
  x when is_reference(x) -> :ref
  x when is_function(x) -> :fun
  x -> x
end))
P.show("compare", {p < q, p == p, p === q, :erlang.phash2(p) == :erlang.phash2(p)})
P.show("map key", %{p => 1, q => 2} |> Map.get(q))
P.show("etf", :erlang.binary_to_term(:erlang.term_to_binary({p, [q]})) == {p, [q]})
P.show("list", p in Port.list() and q in Port.list())

# --- commands and info ---
P.show("echo", P.echo(p, "hello\n"))
send(p, {me, {:command, ["wor", ?l, [?d], "\n"]}})
P.show("send command", P.flush(names))
P.show("info keys", Port.info(p) |> Keyword.keys())
P.show("info", {Port.info(p, :name), Port.info(p, :connected) == {:connected, me}, Port.info(p, :links) == {:links, [me]}})
P.show("counters", {Port.info(p, :input), Port.info(p, :output)})
P.show("os_pid", Port.info(p, :os_pid) |> elem(1) |> is_integer())
P.show("misc info", {Port.info(p, :registered_name), Port.info(p, :queue_size), Port.info(p, :locking)})
P.show("bad item", P.error(fn -> Port.info(p, :bogus) end))
{:links, links} = Process.info(me, :links)
P.show("process links", links |> Enum.filter(&is_port/1) |> Enum.map(&P.name(&1, names)) |> Enum.sort())
P.show("command options", Port.command(p, "x", [:nosuspend]))
P.show("command options data", P.flush(names))

# --- closing ---
P.show("close", Port.close(p))
P.show("after close", P.flush(names))
P.show("info closed", {Port.info(p), Port.info(p, :name)})
P.show("closed command", P.error(fn -> Port.command(p, "x") end))
P.show("closed close", P.error(fn -> Port.close(p) end))
P.show("closed connect", P.error(fn -> Port.connect(p, me) end))
P.show("not a port", {Port.info(:nope), P.error(fn -> Port.command(:nope, "x") end)})
P.show("closed listed", p in Port.list())
send(q, {me, :close})
P.show("close message", P.flush(names))
send(q, {me, :close})
P.show("close message again", P.flush(names))

# --- programs ending ---
sh = fn script, opts -> Port.open({:spawn_executable, "/bin/sh"}, [args: ["-c", script]] ++ opts) end
p = sh.("printf 'a\\nbb'; exit 3", [:binary, :exit_status])
P.show("exit status", P.collect(p))
P.show("exit after status", P.flush(%{p => :p}))
p = sh.("printf abc", [:binary])
P.show("no exit status", P.flush(%{p => :p}))
p = sh.("printf abc", [])
P.show("list data", P.flush(%{p => :p}))
p = sh.("kill -9 $$", [:exit_status])
P.show("killed", P.flush(%{p => :p}))
p = Port.open({:spawn, "sh -c 'exit 5'"}, [:exit_status])
P.show("spawn status", P.flush(%{p => :p}))
p = Port.open({:spawn, "nonexistentcommandxyz"}, [:binary, :exit_status, :stderr_to_stdout])
P.show("missing command", P.collect(p) |> elem(1))
P.flush(%{})
p = sh.("echo out; echo err 1>&2", [:binary, :exit_status, :stderr_to_stdout])
P.show("stderr_to_stdout", P.collect(p))
P.flush(%{})
p = sh.("pwd; echo $FOO; echo ${HOME:-unset}", [:binary, :exit_status, cd: "/", env: [{~c"FOO", ~c"bar"}, {~c"HOME", false}]])
P.show("cd and env", P.collect(p))
P.flush(%{})
p = Port.open({:spawn_executable, "/bin/sh"}, [:binary, :exit_status, args: ["-c", "echo $0"], arg0: "named"])
P.show("arg0", P.collect(p))
P.flush(%{})
p = sh.("echo hidden", [:binary, :exit_status, :hide, :use_stdio, {:parallelism, true}])
P.show("accepted options", P.collect(p))
P.flush(%{})

# --- line mode ---
p = sh.("printf 'a\\nbbbbbb\\nccc'; exit 2", [:binary, :exit_status, {:line, 3}])
P.show("lines", P.flush(%{p => :p}))
p = sh.("printf 'one\\ntwo\\n'", [{:line, 10}])
P.show("lines as lists", P.flush(%{p => :p}))

# --- eof ---
p = sh.("printf abc", [:binary, :eof, :exit_status])
P.show("eof", P.flush(%{p => :p}))
P.show("eof still open", Port.info(p) != nil)
Port.close(p)
P.show("eof closed", P.flush(%{p => :p}))

# --- packets ---
for n <- [1, 2, 4] do
  p = Port.open({:spawn, "cat"}, [:binary, {:packet, n}])
  P.show("packet #{n}", {P.echo(p, "hey"), P.echo(p, ["a", ?b, [?c]]), P.echo(p, "")})
  Port.close(p)
  P.flush(%{})
end

p = sh.("printf '\\000\\003abc\\000\\001z\\000\\005ab'", [{:packet, 2}, :exit_status])
P.show("packets in", P.flush(%{p => :p}))
p = Port.open({:spawn, "cat"}, [{:packet, 1}])
P.show("packet too long", Port.command(p, :binary.copy("x", 256)))
P.show("packet too long closes", P.flush(%{p => :p}))

# --- owners ---
pid =
  spawn(fn ->
    port = Port.open({:spawn, "cat"}, [:binary])
    send(me, {:port, port})
    receive do: (:stop -> :ok)
  end)

p = receive do: ({:port, port} -> port)
P.show("owned elsewhere", Port.info(p, :connected) == {:connected, pid})
send(pid, :stop)
Process.sleep(100)
P.show("owner exited", {Port.info(p), p in Port.list()})

pid =
  spawn(fn ->
    port = Port.open({:spawn, "cat"}, [:binary])
    send(me, {:port, port})
    Process.sleep(:infinity)
  end)

p = receive do: ({:port, port} -> port)
Process.exit(pid, :kill)
Process.sleep(100)
P.show("owner killed", Port.info(p))

p = Port.open({:spawn, "cat"}, [:binary])

other =
  spawn(fn ->
    for _ <- 1..2 do
      receive do: (m -> send(me, {:other_got, m}))
    end
  end)

names = %{p => :p, me => :me, other => :other_pid}
P.show("connect", Port.connect(p, other))
P.show("connected", {Port.info(p, :connected) == {:connected, other}, Port.info(p, :links) |> elem(1) |> Enum.map(&P.name(&1, names)) |> Enum.sort()})
Port.command(p, "to other\n")
P.show("data to new owner", P.flush(names))
send(p, {other, :close})
P.show("closed by new owner", P.flush(names))
P.show("connect errors", {P.error(fn -> Port.connect(Port.open({:spawn, "cat"}, []), :x) end), P.error(fn -> Port.connect(p, me) end)})
P.flush(%{})

p = Port.open({:spawn, "cat"}, [:binary])
names = %{p => :p, me => :me, other => :other_pid}
forward = fn f -> receive do: (m -> send(me, {:other_got, m}); f.(f)) end
other = spawn(fn -> forward.(forward) end)
names = Map.put(names, other, :other_pid)
send(p, {me, {:connect, other}})
P.show("connect message", P.flush(names))
Port.command(p, "x")
P.show("after connect message", P.flush(names))
Port.close(p)
P.flush(%{})

# --- exit signals and links ---
p = Port.open({:spawn, "cat"}, [:binary])
Process.exit(p, :boom)
P.show("exit boom", P.flush(%{p => :p}))
p = Port.open({:spawn, "cat"}, [:binary])
Process.exit(p, :kill)
P.show("exit kill", P.flush(%{p => :p}))
p = Port.open({:spawn, "cat"}, [:binary])
spawn(fn -> Process.exit(p, :normal) end)
P.show("exit normal from other", P.flush(%{p => :p}))
p = Port.open({:spawn, "cat"}, [:binary])

spawn(fn ->
  Process.link(p)
  exit(:crash)
end)

P.show("linked crash", P.flush(%{p => :p}))
p = Port.open({:spawn, "cat"}, [:binary])
spawn(fn -> Process.link(p) end)
P.show("linked normal", {P.flush(%{p => :p}), Port.info(p) != nil})
Process.unlink(p)
P.show("unlinked", Process.info(me, :links) |> elem(1) |> Enum.member?(p))
Port.close(p)
P.show("unlinked close", P.flush(%{p => :p}))
p = Port.open({:spawn, "cat"}, [:binary])
send(p, :garbage)
P.show("badsig", {P.flush(%{p => :p}), Port.info(p) != nil})
spawn(fn -> send(p, {self(), {:command, "x"}}) end)
P.show("badsig from other", P.flush(%{p => :p}))
Port.close(p)
P.flush(%{})
Process.link(p)
P.show("link closed", P.flush(%{p => :p}))

# --- monitors ---
p = Port.open({:spawn, "cat"}, [:binary])
ref = Port.monitor(p)
names = %{p => :p, ref => :ref}
P.show("monitors", {Process.info(me, :monitors) == {:monitors, [{:port, p}]}, Port.info(p, :monitored_by) == {:monitored_by, [me]}})
Port.close(p)
P.show("down", P.flush(names))
ref = Port.monitor(p)
P.show("down closed", P.flush(Map.put(names, ref, :ref)))
p = Port.open({:spawn, "cat"}, [:binary])
ref = Port.monitor(p)
P.show("demonitor", Port.demonitor(ref))
Port.close(p)
P.show("demonitored close", P.flush(%{p => :p}))

# --- untrapped owner ---
Process.flag(:trap_exit, false)
p = sh.("printf done", [:binary])
Process.sleep(100)
P.show("untrapped exit", P.flush(%{p => :p}))
Process.flag(:trap_exit, true)

# --- open errors ---
P.show("enoent", P.error(fn -> Port.open({:spawn_executable, "/nonexistent/program"}, []) end))
P.show("eacces", P.error(fn -> Port.open({:spawn_executable, "/etc/hosts"}, []) end))
P.show("bad option", P.error(fn -> Port.open({:spawn, "cat"}, [:bogus]) end))
P.show("bad packet", P.error(fn -> Port.open({:spawn, "cat"}, [{:packet, 3}]) end))
P.show("bad name", P.error(fn -> Port.open({:bogus, "cat"}, []) end))
P.show("bad options", P.error(fn -> Port.open({:spawn, "cat"}, :nope) end))

# --- System.cmd ---
P.show("cmd", System.cmd("echo", ["hello", "world"]))
P.show("cmd absolute", System.cmd("/bin/echo", ["abs"]))
P.show("cmd status", System.cmd("sh", ["-c", "echo out; exit 7"]))
P.show("cmd stderr", System.cmd("sh", ["-c", "echo out; echo err 1>&2"], stderr_to_stdout: true))
P.show("cmd env", System.cmd("sh", ["-c", "echo $A-$B"], env: %{"A" => "1", "B" => nil}))
P.show("cmd cd", System.cmd("pwd", [], cd: "/"))
P.show("cmd into", System.cmd("printf", ["a\nb\nc"], into: []) |> then(fn {l, s} -> {IO.iodata_to_binary(l), s} end))
P.show("cmd lines", System.cmd("printf", ["a\nbb\nccc"], lines: 2, into: []))
P.show("cmd arg0", System.cmd("sh", ["-c", "echo $0"], arg0: "zzz"))
P.show("cmd parallelism", System.cmd("echo", ["p"], parallelism: true))
P.show("cmd stream", System.cmd("echo", ["streamed"], into: IO.stream()) |> elem(1))
P.show("cmd nouse_stdio", System.cmd("echo", ["direct"], use_stdio: false))
P.show("cmd missing", P.error(fn -> System.cmd("nonexistentcommandxyz", []) end))
P.show("cmd missing path", P.error(fn -> System.cmd("/nonexistent/x", []) end))
P.show("cmd bad option", P.error(fn -> System.cmd("echo", [], foo: 1) end))
P.show("cmd bad args", P.error(fn -> System.cmd("echo", [:a]) end))
P.show("cmd bad env", P.error(fn -> System.cmd("echo", [], env: [:x]) end))
P.show("cmd null byte", P.error(fn -> System.cmd("ec\0ho", []) end))
P.show("cmd stdio conflict", P.error(fn -> System.cmd("echo", [], use_stdio: false, stderr_to_stdout: true) end))
P.show("shell", System.shell("echo a; echo b 1>&2", stderr_to_stdout: true))
P.show("shell status", System.shell("exit 4"))
P.show("shell empty", System.shell("   "))
P.show("shell comment", System.shell("echo c # trailing comment"))
P.show("shell stdin", System.shell("cat", close_stdin: true))
P.show("cmd port exits", P.flush(%{}))

# --- :os.cmd ---
P.show("os.cmd", :os.cmd(~c"echo hi; echo err 1>&2"))
P.show("os.cmd status", :os.cmd(~c"exit 3"))
P.show("os.cmd unicode", :os.cmd(~c"printf 'é'"))
P.show("os.cmd atom", :os.cmd(:"echo atom"))
P.show("os.cmd deep", :os.cmd([~c"echo ", [~c"deep"]]))
P.show("os.cmd max_size", :os.cmd(~c"echo 123456789", %{max_size: 4}))
P.show("os.cmd background", :os.cmd(~c"sleep 5 & echo quick"))
P.show("os.cmd binary", P.error(fn -> :os.cmd("echo bin") end))
P.show("os.cmd stray messages", P.flush(%{}))
