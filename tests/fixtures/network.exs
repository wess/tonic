defmodule NetworkParity do
  @dir "tests/cases/ssl/"
  @lo {127, 0, 0, 1}

  def rejected(fun) do
    try do
      case fun.() do
        {:error, _} -> true
        _ -> false
      end
    rescue
      _ -> true
    catch
      _, _ -> true
    end
  end

  def malformed do
    invalid = [mode: :invalid]
    improper = [{:active, false} | :invalid]
    IO.inspect({rejected(fn -> :gen_tcp.listen(0, invalid) end), rejected(fn -> :gen_tcp.listen(0, improper) end)}, label: "tcp malformed options")
    IO.inspect({rejected(fn -> :gen_udp.open(0, invalid) end), rejected(fn -> :gen_udp.open(0, improper) end)}, label: "udp malformed options")
    IO.inspect({rejected(fn -> :ssl.listen(0, invalid) end), rejected(fn -> :ssl.listen(0, improper) end)}, label: "tls malformed options")
  end

  def tcp do
    for options <- [[], [:binary], [packet: 1], [:binary, packet: 2], [:binary, packet: 4], [:binary, packet: :line]] do
      {:ok, listener} = :gen_tcp.listen(0, [ip: @lo, active: false, reuseaddr: true, nodelay: true, keepalive: true] ++ options)
      {:ok, port} = :inet.port(listener)
      parent = self()
      server = spawn(fn ->
        {:ok, socket} = :gen_tcp.accept(listener, 2000)
        {:ok, data} = :gen_tcp.recv(socket, 0, 2000)
        :ok = :gen_tcp.send(socket, data)
        send(parent, {:tcp_server, self()})
        receive do :close -> :gen_tcp.close(socket) end
      end)
      {:ok, socket} = :gen_tcp.connect(~c"localhost", port,
        [active: false, ip: @lo, nodelay: true, keepalive: true, tos: 16] ++ options, 2000)
      IO.inspect(:inet.getopts(socket, [:mode, :packet, :nodelay, :keepalive, :tos]), label: "tcp opts")
      true = rejected(fn -> :inet.setopts(socket, [{:nodelay, false} | :invalid]) end)
      true = rejected(fn -> :inet.getopts(socket, [:mode | :invalid]) end)
      :ok = :inet.setopts(socket, nodelay: false, keepalive: false, tos: 8)
      {:ok, [nodelay: false, keepalive: false, tos: 8]} = :inet.getopts(socket, [:nodelay, :keepalive, :tos])
      :ok = :gen_tcp.send(socket, "payload\n")
      IO.inspect(:gen_tcp.recv(socket, 0, 2000), label: "tcp echo")
      receive do {:tcp_server, ^server} -> :ok after 2000 -> raise "tcp server did not complete" end
      IO.inspect(:inet.setopts(socket, unsupported: true), label: "tcp bad setopts")
      IO.inspect(:inet.getopts(socket, [:unsupported]), label: "tcp bad getopts")
      :gen_tcp.close(socket)
      send(server, :close)
      :gen_tcp.close(listener)
    end

    {:ok, listener} = :gen_tcp.listen(0, [:binary, active: false, packet: 2])
    {:ok, port} = :inet.port(listener)
    parent = self()
    server = spawn(fn ->
      {:ok, socket} = :gen_tcp.accept(listener)
      :ok = :gen_tcp.send(socket, "first")
      receive do :next -> :ok end
      :ok = :gen_tcp.send(socket, "second")
      receive do :close -> :gen_tcp.close(socket) end
      send(parent, :tcp_closed)
    end)
    {:ok, socket} = :gen_tcp.connect(@lo, port, [:binary, active: 2, packet: 2])
    receive do {:tcp, ^socket, first} -> IO.inspect(first, label: "tcp active first") after 2000 -> raise "no first packet" end
    send(server, :next)
    receive do {:tcp, ^socket, second} -> IO.inspect(second, label: "tcp active second") after 2000 -> raise "no second packet" end
    receive do {:tcp_passive, ^socket} -> IO.puts("tcp passive") after 2000 -> raise "no passive notification" end
    {:ok, [active: false]} = :inet.getopts(socket, [:active])
    :gen_tcp.close(socket)
    send(server, :close)
    receive do :tcp_closed -> :ok after 2000 -> raise "tcp server did not close" end
    :gen_tcp.close(listener)
  end

  def udp do
    {:ok, receiver} = :gen_udp.open(0, [:binary, ip: @lo, active: false])
    {:ok, port} = :inet.port(receiver)
    {:ok, sender} = :gen_udp.open(0, [:binary, ip: @lo, active: false, ttl: 42, tos: 16,
      multicast_ttl: 3, multicast_loop: false, dontroute: true, buffer: 16384, read_packets: 2])
    IO.inspect(:inet.getopts(sender, [:ttl, :tos, :multicast_ttl, :multicast_loop, :dontroute, :buffer, :read_packets]))
    true = rejected(fn -> :inet.setopts(sender, [{:ttl, 31} | :invalid]) end)
    true = rejected(fn -> :inet.getopts(sender, [:ttl | :invalid]) end)
    :ok = :inet.setopts(sender, ttl: 31, tos: 8, multicast_ttl: 4, multicast_loop: true, dontroute: false)
    IO.inspect(:inet.getopts(sender, [:ttl, :tos, :multicast_ttl, :multicast_loop, :dontroute]))
    :ok = :gen_udp.connect(sender, ~c"localhost", port)
    {:ok, {@lo, ^port}} = :inet.peername(sender)
    :ok = :gen_udp.send(sender, ["con", ["nected"]])
    {:ok, {@lo, source, "connected"}} = :gen_udp.recv(receiver, 0, 1000)
    :ok = :gen_udp.send(receiver, @lo, source, "reply")
    IO.inspect(:gen_udp.recv(sender, 0, 1000) |> elem(1) |> elem(2))
    {:ok, stranger} = :gen_udp.open(0, [ip: @lo, active: false])
    :ok = :gen_udp.send(stranger, @lo, source, "discard")
    IO.inspect(:gen_udp.recv(sender, 0, 30))
    IO.inspect(:inet.setopts(sender, unsupported: true))
    IO.inspect(:inet.getopts(sender, [:unsupported]))
    Enum.each([sender, receiver, stranger], &:gen_udp.close/1)

    {:ok, multicast} = :gen_udp.open(0, [:binary, active: false, reuseaddr: true,
      multicast_loop: true, multicast_if: @lo, add_membership: {{239, 255, 42, 1}, @lo}])
    {:ok, multicast_port} = :inet.port(multicast)
    :ok = :gen_udp.send(multicast, {239, 255, 42, 1}, multicast_port, "multicast")
    IO.inspect(:gen_udp.recv(multicast, 0, 1000) |> elem(1) |> elem(2))
    :ok = :inet.setopts(multicast, drop_membership: {{239, 255, 42, 1}, @lo})
    :gen_udp.close(multicast)
  end

  def exchange(client_extra, server_extra \\ []) do
    server_opts = [certfile: @dir <> "server.pem", keyfile: @dir <> "serverkey.pem", active: false, mode: :binary, log_level: :none]
    {:ok, listener} = :ssl.listen(0, Keyword.merge(server_opts, server_extra))
    {:ok, {_, port}} = :ssl.sockname(listener)
    parent = self()
    server = spawn(fn ->
      {:ok, pending} = :ssl.transport_accept(listener, 2000)
      result = case :ssl.handshake(pending, 2000) do
        {:ok, socket} ->
          {:ok, "ping"} = :ssl.recv(socket, 0, 2000)
          :ok = :ssl.send(socket, "pong")
          :ssl.close(socket)
          :ok
        {:error, _} -> :error
      end
      send(parent, {:server, self(), result})
    end)
    result = case :ssl.connect(~c"localhost", port,
      Keyword.merge([active: false, mode: :binary, verify: :verify_peer, cacertfile: @dir <> "ca.pem", log_level: :none, nodelay: true, keepalive: true], client_extra), 2000) do
      {:ok, socket} ->
            true = rejected(fn -> :ssl.setopts(socket, mode: :invalid) end)
            true = rejected(fn -> :ssl.setopts(socket, [{:nodelay, false} | :invalid]) end)
            true = rejected(fn -> :ssl.getopts(socket, [:mode | :invalid]) end)
            {:ok, [nodelay: true, keepalive: true]} = :ssl.getopts(socket, [:keepalive, :nodelay])
            :ok = :ssl.setopts(socket, nodelay: false, keepalive: false)
            {:ok, [keepalive: false, nodelay: false]} = :ssl.getopts(socket, [:nodelay, :keepalive])
        :ok = :ssl.send(socket, "ping")
        result = case :ssl.recv(socket, 0, 2000) do
          {:ok, response} ->
            {:ok, info} = :ssl.connection_information(socket, [:selected_cipher_suite])
            {response, info}
          {:error, _} -> :error
        end
        :ssl.close(socket)
        result
      {:error, _} -> :error
    end
    receive do
      {:server, ^server, _} -> :ok
    after
      3000 -> raise "server did not complete"
    end
    :ssl.close(listener)
    result
  end

  def tls do
    :ssl.start()
    parent = self()
    verify = fn _cert, event, state ->
      event = case event do
        {:extension, {:Extension, oid, _, _}} -> {:extension, oid}
        other -> other
      end
      send(parent, {:verification, event, state})
      {:valid, state + 1}
    end
    IO.inspect(exchange([verify_fun: {verify, 0}], []), label: "verified")
    IO.inspect(events(), label: "events")
    IO.inspect(exchange([certfile: @dir <> "client.pem", keyfile: @dir <> "clientkey.pem"],
      [verify: :verify_peer, cacertfile: @dir <> "ca.pem", fail_if_no_peer_cert: true, verify_fun: {verify, 10}]), label: "mutual")
    IO.inspect(events(), label: "mutual events")
    partial = fn chain ->
      send(parent, {:partial, length(chain)})
      :unknown_ca
    end
    IO.inspect(exchange([partial_chain: partial], []), label: "partial")
    receive do {:partial, count} -> IO.inspect(count, label: "partial length") after 1000 -> raise "missing partial chain callback" end
    IO.inspect(exchange([verify_fun: {fn _, _, state -> {:fail, state} end, :rejected}], []), label: "verify rejection")
    hostname = fn reference, presented ->
      send(parent, {:match, reference, presented})
      :default
    end
    IO.inspect(exchange([customize_hostname_check: [match_fun: hostname]], []), label: "hostname")
    IO.inspect(matches(), label: "matches")
    IO.inspect(exchange([customize_hostname_check: [match_fun: fn _, _ -> false end]], []), label: "hostname rejection")
    sni = fn hostname ->
      send(parent, {:sni, hostname})
      [certfile: @dir <> "server.pem", keyfile: @dir <> "serverkey.pem"]
    end
    IO.inspect(exchange([], [sni_fun: sni]), label: "sni")
    receive do {:sni, name} -> IO.inspect(name, label: "sni name") after 1000 -> raise "missing sni callback" end
    suite = %{key_exchange: :any, cipher: :aes_128_gcm, mac: :aead, prf: :sha256}
    IO.inspect(exchange([ciphers: [suite], versions: [:"tlsv1.3"]], []), label: "cipher")
    IO.inspect(exchange([server_name_indication: :disable], []), label: "disabled hostname")
    strong = %{key_exchange: :any, cipher: :aes_256_gcm, mac: :aead, prf: :sha384}
    IO.inspect(exchange([ciphers: [strong, suite]], [ciphers: [suite, strong], honor_cipher_order: true]), label: "server cipher order")
    IO.inspect(exchange([ciphers: [strong, suite]], [ciphers: [suite, strong], honor_cipher_order: false]), label: "client cipher order")
  end

  def events do
    receive do
      {:verification, event, state} -> [{event, state} | events()]
    after 0 -> [] end
  end

  def matches do
    receive do
      {:match, reference, presented} -> [{reference, presented} | matches()]
    after 0 -> [] end
  end
end
NetworkParity.malformed()
NetworkParity.tcp()
NetworkParity.udp()
NetworkParity.tls()
