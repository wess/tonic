defmodule NetworkCancel do
  @dir "tests/cases/ssl/"

  def listener do
    {:ok, listener} = :ssl.listen(0, certfile: @dir <> "server.pem", keyfile: @dir <> "serverkey.pem", active: false)
    {:ok, {_, port}} = :ssl.sockname(listener)
    {listener, port}
  end

  def down(pid) do
    monitor = Process.monitor(pid)
    receive do
      {:DOWN, ^monitor, :process, ^pid, _} -> :ok
    after
      2000 -> raise "callback process survived cancellation"
    end
  end

  def client_timeout do
    parent = self()
    {listener, port} = listener()
    spawn(fn ->
      {:ok, socket} = :ssl.transport_accept(listener, 2000)
      :ssl.handshake(socket, 2000)
    end)
    verify = fn _, _, _ ->
      send(parent, {:verify, self()})
      Process.sleep(:infinity)
    end
    {:error, :timeout} = :ssl.connect(~c"localhost", port, [active: false, verify: :verify_peer,
      cacertfile: @dir <> "ca.pem", verify_fun: {verify, nil}], 100)
    callback = receive do {:verify, callback} -> callback after 2000 -> raise "missing callback" end
    down(callback)
    :ssl.close(listener)
    IO.puts("callback timeout cleaned")
  end

  def owner_exit do
    parent = self()
    {listener, port} = listener()
    spawn(fn ->
      {:ok, socket} = :ssl.transport_accept(listener, 2000)
      :ssl.handshake(socket, 2000)
    end)
    verify = fn _, _, _ ->
      send(parent, {:verify, self()})
      Process.sleep(:infinity)
    end
    owner = spawn(fn ->
      :ssl.connect(~c"localhost", port,
        [active: false, verify: :verify_peer, cacertfile: @dir <> "ca.pem", verify_fun: {verify, nil}], :infinity)
    end)
    callback = receive do {:verify, callback} -> callback after 2000 -> raise "missing callback" end
    Process.exit(owner, :kill)
    down(owner)
    down(callback)
    :ssl.close(listener)
    IO.puts("callback owner exit cleaned")
  end

  def server_close do
    parent = self()
    sni = fn _ ->
      send(parent, {:sni, self()})
      Process.sleep(:infinity)
    end
    {:ok, listener} = :ssl.listen(0, certfile: @dir <> "server.pem", keyfile: @dir <> "serverkey.pem",
      active: false, sni_fun: sni)
    {:ok, {_, port}} = :ssl.sockname(listener)
    server = spawn(fn ->
      {:ok, socket} = :ssl.transport_accept(listener, 2000)
      send(parent, {:accepted, socket})
      {:error, _} = :ssl.handshake(socket, :infinity)
      send(parent, :server_closed)
    end)
    client = spawn(fn ->
      {:error, _} = :ssl.connect(~c"localhost", port, [active: false, verify: :verify_none], 2000)
    end)
    socket = receive do {:accepted, socket} -> socket after 2000 -> raise "no accepted socket" end
    callback = receive do {:sni, callback} -> callback after 2000 -> raise "missing sni callback" end
    :ssl.close(socket)
    receive do :server_closed -> :ok after 2000 -> raise "closed handshake did not return" end
    down(callback)
    down(server)
    down(client)
    :ssl.close(listener)
    IO.puts("callback socket close cleaned")
  end
end
:ssl.start()
NetworkCancel.client_timeout()
NetworkCancel.owner_exit()
NetworkCancel.server_close()
