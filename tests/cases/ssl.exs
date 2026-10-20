# TLS over localhost with both ends in the same program. The certificates in
# tests/cases/ssl/ are throwaway test material (P-256, valid until 2126), made with:
#
#   openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 36500 \
#     -keyout cakey.pem -out ca.pem -subj "/CN=Tonic Test CA" -extensions v3 \
#     -config <(printf "[req]\ndistinguished_name=dn\n[dn]\n[v3]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\nsubjectKeyIdentifier=hash\n")
#   (other.pem: the same with "/CN=Tonic Other CA", its key discarded)
#   openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
#     -keyout serverkey.pem -out server.csr -subj "/CN=localhost"
#   openssl x509 -req -in server.csr -CA ca.pem -CAkey cakey.pem -CAcreateserial -days 36500 \
#     -out server.pem -extfile <(printf "subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n")
#   (client.pem/clientkey.pem: the same with "/CN=tonic client" and extendedKeyUsage=clientAuth)

defmodule SslCase do
  @dir "tests/cases/ssl/"
  @quiet [log_level: :none]

  def server_opts(extra \\ []) do
    [certfile: @dir <> "server.pem", keyfile: @dir <> "serverkey.pem", reuseaddr: true, active: false] ++
      @quiet ++ extra
  end

  def client_opts(extra \\ []) do
    [verify: :verify_peer, cacertfile: @dir <> "ca.pem", active: false] ++ @quiet ++ extra
  end

  def listen(extra \\ []) do
    {:ok, l} = :ssl.listen(0, server_opts(extra))
    {:ok, {_, port}} = :ssl.sockname(l)
    {l, port}
  end

  # Accepts one connection in a new process, which runs `fun` on the socket
  # and reports back.
  def serve(l, fun, handshake \\ []) do
    parent = self()

    spawn(fn ->
      {:ok, t} = :ssl.transport_accept(l, 5000)

      result =
        case handshake do
          [] -> :ssl.handshake(t, 5000)
          opts -> :ssl.handshake(t, opts, 5000)
        end

      case result do
        {:ok, s} -> send(parent, {:served, fun.(s)})
        {:error, reason} -> send(parent, {:served, {:handshake, short(reason)}})
      end
    end)
  end

  def served do
    receive do
      {:served, r} -> r
    after
      5000 -> :served_timeout
    end
  end

  def short({:tls_alert, {alert, text}}) when is_list(text), do: {:tls_alert, alert}
  def short(reason), do: reason

  def connect(port, extra \\ []) do
    case :ssl.connect(~c"localhost", port, client_opts(extra), 5000) do
      {:ok, s} -> {:ok, s}
      {:error, reason} -> {:error, short(reason)}
    end
  end

  def flush do
    receive do
      m -> [m | flush()]
    after
      100 -> []
    end
  end

  def echo(s) do
    case :ssl.recv(s, 0, 2000) do
      {:ok, data} ->
        :ok = :ssl.send(s, data)
        echo(s)

      other ->
        other
    end
  end

  def run do
    IO.inspect(Application.ensure_all_started(:ssl), label: "start")
    IO.inspect(:ssl.start(), label: "ssl.start")
    versions = :ssl.versions()
    IO.inspect(Keyword.get(versions, :supported), label: "versions")

    {l, port} = listen(alpn_preferred_protocols: ["h2", "http/1.1"])
    IO.inspect(is_integer(port) and port > 0, label: "listen port")

    # verify_peer, passive, list mode by default
    serve(l, fn s ->
      {:ok, data} = :ssl.recv(s, 0, 2000)
      :ok = :ssl.send(s, ["po", ?n, "g"])
      {:ok, [protocol: p]} = :ssl.connection_information(s, [:protocol])
      {:ok, alpn} = :ssl.negotiated_protocol(s)
      {:ok, {peer, _}} = :ssl.peername(s)
      _ = :ssl.recv(s, 0, 2000)
      {data, p, alpn, peer}
    end)

    {:ok, c} = connect(port, alpn_advertised_protocols: ["h2"])
    IO.inspect(:ssl.getopts(c, [:active, :mode, :packet]), label: "getopts")
    IO.inspect(:ssl.negotiated_protocol(c), label: "alpn")
    {:ok, info} = :ssl.connection_information(c, [:protocol, :selected_cipher_suite])
    IO.inspect(info[:protocol], label: "protocol")
    IO.inspect(Enum.sort(Map.keys(info[:selected_cipher_suite])), label: "suite keys")
    {:ok, all} = :ssl.connection_information(c)
    IO.inspect(Keyword.take(all, [:protocol]), label: "info/1")
    {:ok, der} = :ssl.peercert(c)
    IO.inspect(is_binary(der) and binary_part(der, 0, 1) == <<0x30>>, label: "peercert der")
    {:ok, {ip, cport}} = :ssl.sockname(c)
    IO.inspect({ip, is_integer(cport)}, label: "sockname")
    {:ok, {pip, pport}} = :ssl.peername(c)
    IO.inspect({pip, pport == port}, label: "peername")
    :ok = :ssl.send(c, "ping")
    IO.inspect(:ssl.recv(c, 0, 2000), label: "recv list")
    :ok = :ssl.close(c)
    IO.inspect(served(), label: "server saw")
    IO.inspect(:ssl.close(c), label: "close again")
    IO.inspect(:ssl.send(c, "x"), label: "send closed")
    IO.inspect(:ssl.recv(c, 0), label: "recv closed")

    # no ALPN overlap
    serve(l, fn _ -> :unexpected end)
    IO.inspect(connect(port, verify: :verify_none, alpn_advertised_protocols: ["spdy/3"]), label: "no alpn")
    IO.inspect(served(), label: "server no alpn")

    # untrusted CA
    serve(l, fn _ -> :unexpected end)
    IO.inspect(connect(port, cacertfile: @dir <> "other.pem"), label: "unknown ca")
    IO.inspect(served(), label: "server unknown ca")

    # wrong host name
    serve(l, fn _ -> :unexpected end)
    IO.inspect(connect(port, server_name_indication: ~c"wrong.example"), label: "hostname")
    served()

    # cacerts as DER, hostname match fun, TLS 1.2
    [{:Certificate, ca_der, :not_encrypted}] = :public_key.pem_decode(File.read!(@dir <> "ca.pem"))
    serve(l, fn s -> :ssl.recv(s, 0, 2000) end)

    {:ok, c} =
      :ssl.connect(~c"localhost", port,
        verify: :verify_peer,
        active: false,
        log_level: :none,
        cacerts: [ca_der],
        versions: [:"tlsv1.2"],
        depth: 3,
        reuse_sessions: false,
        customize_hostname_check: [match_fun: :public_key.pkix_verify_hostname_match_fun(:https)]
      )


    {:ok, [protocol: p, selected_cipher_suite: suite]} =
      :ssl.connection_information(c, [:protocol, :selected_cipher_suite])

    IO.inspect({p, suite.key_exchange, suite.mac}, label: "tls1.2")
    IO.inspect(:ssl.negotiated_protocol(c), label: "alpn none")
    :ssl.send(c, "twelve")
    IO.inspect(served(), label: "server tls1.2")
    :ssl.close(c)

    # verify_none without trust anchors; missing verify setting
    serve(l, fn s -> :ssl.recv(s, 0, 2000) end)
    {:ok, c} = :ssl.connect(~c"localhost", port, [verify: :verify_none, active: false, log_level: :none])
    :ssl.send(c, "unverified")
    IO.inspect(served(), label: "verify_none")
    :ssl.close(c)
    IO.inspect(:ssl.connect(~c"localhost", port, [log_level: :none], 5000), label: "no verify opt")

    # binary active modes
    serve(l, &echo/1)
    {:ok, c} = connect(port, [:binary, active: true])
    :ssl.send(c, "one")
    IO.inspect(receive(do: (m -> m)) |> elem(0), label: "active true tag")
    :ok = :ssl.setopts(c, active: :once)
    :ssl.send(c, "two")
    receive do
      {:ssl, ^c, data} -> IO.inspect(data, label: "once")
    end
    IO.inspect(:ssl.getopts(c, [:active]), label: "after once")
    :ssl.send(c, "three")
    Process.sleep(100)
    IO.inspect(flush(), label: "nothing while passive")
    IO.inspect(:ssl.recv(c, 0, 1000), label: "recv passive")
    :ok = :ssl.setopts(c, active: 2)
    :ssl.send(c, "a")
    receive do: ({:ssl, ^c, d} -> IO.inspect(d, label: "n1"))
    :ssl.send(c, "b")
    receive do: ({:ssl, ^c, d} -> IO.inspect(d, label: "n2"))
    receive do: ({:ssl_passive, ^c} -> IO.puts("ssl_passive"))
    IO.inspect(:ssl.getopts(c, [:active]), label: "after n")
    :ok = :ssl.setopts(c, active: true)
    IO.inspect(:ssl.recv(c, 0, 100), label: "recv while active")
    :ssl.close(c)
    IO.inspect(served(), label: "echo end")

    # recv timeout keeps the connection usable
    serve(l, fn s ->
      Process.sleep(300)
      :ssl.send(s, "late")
      :ssl.recv(s, 0, 2000)
    end)

    {:ok, c} = connect(port, [:binary])
    IO.inspect(:ssl.recv(c, 0, 50), label: "timeout")
    IO.inspect(:ssl.recv(c, 0, 2000), label: "after timeout")
    :ssl.close(c)
    served()

    # peer closes: active client gets ssl_closed, passive gets :closed
    serve(l, fn s -> :ssl.close(s) end)
    {:ok, c} = connect(port, [:binary, active: true])
    receive do
      {:ssl_closed, ^c} -> IO.puts("ssl_closed")
    after
      2000 -> IO.puts("no ssl_closed")
    end
    served()

    serve(l, fn s -> :ssl.send(s, "bye"); :ssl.close(s) end)
    {:ok, c} = connect(port, [:binary])
    Process.sleep(100)
    IO.inspect(:ssl.recv(c, 0, 2000), label: "data then")
    IO.inspect(:ssl.recv(c, 0, 2000), label: "closed")
    served()

    # packet framing
    for packet <- [1, 2, 4] do
      serve(l, fn s ->
        :ssl.setopts(s, packet: packet)
        {:ok, a} = :ssl.recv(s, 0, 2000)
        {:ok, b} = :ssl.recv(s, 0, 2000)
        {a, b}
      end)

      {:ok, c} = connect(port, [:binary, packet: packet])
      :ssl.send(c, "first")
      :ssl.send(c, "second message")
      IO.inspect(served(), label: "packet #{packet}")
      :ssl.close(c)
    end

    serve(l, fn s ->
      :ssl.send(s, "line one\nline two\npartial")
      :ssl.recv(s, 0, 2000)
    end)

    {:ok, c} = connect(port, [:binary, packet: :line, active: true])
    for _ <- 1..2, do: receive(do: ({:ssl, ^c, line} -> IO.inspect(line, label: "line")))
    IO.inspect(:ssl.getopts(c, [:packet]), label: "line opt")
    :ssl.close(c)
    served()

    serve(l, fn s -> :ssl.recv(s, 0, 2000) end, [])
    {:ok, c} = connect(port, packet: :raw)
    :ssl.setopts(c, packet: 2, mode: :binary)
    IO.inspect(:ssl.getopts(c, [:packet, :mode]), label: "packet 2 opts")
    :ssl.send(c, "framed")
    IO.inspect(served(), label: "raw server sees prefix")
    :ssl.close(c)

    # 1 MB both ways
    blob = :binary.copy(<<1, 2, 3, 4, 5, 6, 7, 8>>, 131_072)

    serve(l, fn s ->
      :ok = :ssl.setopts(s, [:binary])
      got = recv_all(s, byte_size(blob), [])
      :ok = :ssl.send(s, got)
      byte_size(got) == byte_size(blob) and got == blob
    end)

    {:ok, c} = connect(port, [:binary])
    :ok = :ssl.send(c, blob)
    back = recv_all(c, byte_size(blob), [])
    IO.inspect({byte_size(back), back == blob}, label: "1MB back")
    IO.inspect(served(), label: "1MB server")
    :ssl.close(c)

    # controlling_process and owner exit
    serve(l, fn s -> :ssl.send(s, "for the new owner"); :ssl.recv(s, 0, 2000) end)
    {:ok, c} = connect(port, [:binary, active: false])
    me = self()

    owner =
      spawn(fn ->
        receive do
          :go ->
            :ok = :ssl.setopts(c, active: :once)
            receive do: ({:ssl, ^c, d} -> send(me, {:owner_got, d}))
            receive do: (:stop -> :ok)
        end
      end)

    IO.inspect(:ssl.controlling_process(c, owner), label: "controlling_process")
    send(owner, :go)
    receive do: ({:owner_got, d} -> IO.inspect(d, label: "new owner got"))
    ref = Process.monitor(owner)
    send(owner, :stop)
    receive do: ({:DOWN, ^ref, _, _, _} -> :ok)
    Process.sleep(100)
    IO.inspect(served(), label: "server after owner exit")
    IO.inspect(:ssl.send(c, "late"), label: "send after owner exit")

    # client certificates
    {l2, port2} = listen(verify: :verify_peer, fail_if_no_peer_cert: true, cacertfile: @dir <> "ca.pem")
    serve(l2, fn s -> {:ok, der} = :ssl.peercert(s); {:ok, d} = :ssl.recv(s, 0, 2000); {is_binary(der), d} end)
    {:ok, c} = connect(port2, [:binary, certfile: @dir <> "client.pem", keyfile: @dir <> "clientkey.pem"])
    :ssl.send(c, "with cert")
    IO.inspect(served(), label: "client cert")
    :ssl.close(c)

    serve(l2, fn _ -> :unexpected end)
    result =
      case connect(port2, [:binary]) do
        {:ok, c} -> :ssl.recv(c, 0, 2000)
        other -> other
      end
    IO.inspect(match?({:error, _}, result), label: "no client cert fails")
    IO.inspect(served(), label: "server without cert")

    # upgrading a gen_tcp socket
    serve(l, fn s -> :ssl.recv(s, 0, 2000) end)
    {:ok, tcp} = :gen_tcp.connect(~c"localhost", port, [:binary, active: false])
    {:ok, c} = :ssl.connect(tcp, client_opts(server_name_indication: ~c"localhost"), 5000)
    :ssl.send(c, "upgraded")
    IO.inspect(served(), label: "upgrade")
    :ssl.close(c)

    # errors
    IO.inspect(connect(1), label: "refused")
    :ok = :ssl.close(l)
    IO.inspect(:ssl.transport_accept(l, 100), label: "accept closed")
    IO.inspect(:ssl.cipher_suites(:default, :"tlsv1.3") |> Enum.all?(&is_map/1), label: "suites")
    IO.inspect(length(:ssl.cipher_suites(:all, :"tlsv1.2")) > 5, label: "suites 1.2")
  end

  def recv_all(_s, 0, acc), do: IO.iodata_to_binary(Enum.reverse(acc))

  def recv_all(s, left, acc) do
    {:ok, d} = :ssl.recv(s, 0, 5000)
    recv_all(s, left - byte_size(d), [d | acc])
  end
end

SslCase.run()
