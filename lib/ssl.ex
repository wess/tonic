defmodule :ssl do
  # TLS over the runtime's rustls sockets (runtime/src/tls). Waiting
  # operations reply with {:"$tonic_ssl", ref, result}. Options are
  # normalized here; the runtime reads the plain forms.

  @tls13 [
    {"TLS13_AES_256_GCM_SHA384", %{key_exchange: :any, cipher: :aes_256_gcm, mac: :aead, prf: :sha384}, ~c"TLS_AES_256_GCM_SHA384"},
    {"TLS13_AES_128_GCM_SHA256", %{key_exchange: :any, cipher: :aes_128_gcm, mac: :aead, prf: :sha256}, ~c"TLS_AES_128_GCM_SHA256"},
    {"TLS13_CHACHA20_POLY1305_SHA256", %{key_exchange: :any, cipher: :chacha20_poly1305, mac: :aead, prf: :sha256},
     ~c"TLS_CHACHA20_POLY1305_SHA256"}
  ]

  @tls12 [
    {"TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384", %{key_exchange: :ecdhe_ecdsa, cipher: :aes_256_gcm, mac: :aead, prf: :sha384},
     ~c"ECDHE-ECDSA-AES256-GCM-SHA384"},
    {"TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384", %{key_exchange: :ecdhe_rsa, cipher: :aes_256_gcm, mac: :aead, prf: :sha384},
     ~c"ECDHE-RSA-AES256-GCM-SHA384"},
    {"TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
     %{key_exchange: :ecdhe_ecdsa, cipher: :chacha20_poly1305, mac: :aead, prf: :sha256}, ~c"ECDHE-ECDSA-CHACHA20-POLY1305"},
    {"TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
     %{key_exchange: :ecdhe_rsa, cipher: :chacha20_poly1305, mac: :aead, prf: :sha256}, ~c"ECDHE-RSA-CHACHA20-POLY1305"},
    {"TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256", %{key_exchange: :ecdhe_ecdsa, cipher: :aes_128_gcm, mac: :aead, prf: :sha256},
     ~c"ECDHE-ECDSA-AES128-GCM-SHA256"},
    {"TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256", %{key_exchange: :ecdhe_rsa, cipher: :aes_128_gcm, mac: :aead, prf: :sha256},
     ~c"ECDHE-RSA-AES128-GCM-SHA256"}
  ]

  ## Application

  def start, do: start(:temporary)

  def start(type) do
    case :application.ensure_all_started(:ssl, type) do
      {:ok, _} -> :ok
      other -> other
    end
  end

  def stop, do: :application.stop(:ssl)

  def versions do
    [
      ssl_app: ~c"11.2.12",
      supported: [:"tlsv1.3", :"tlsv1.2"],
      supported_dtls: [],
      available: [:"tlsv1.3", :"tlsv1.2"],
      available_dtls: [],
      implemented: [:"tlsv1.3", :"tlsv1.2"],
      implemented_dtls: []
    ]
  end

  def clear_pem_cache, do: :ok

  ## Cipher suites

  def cipher_suites(description, version), do: cipher_suites(description, version, :default)

  def cipher_suites(description, version, format) when description in [:default, :all, :exclusive] do
    suites =
      case version do
        :"tlsv1.3" -> @tls13
        :"tlsv1.2" when description == :exclusive -> @tls12
        :"tlsv1.2" -> @tls12
        {3, 4} -> @tls13
        {3, 3} -> @tls12
        _ -> []
      end

    Enum.map(suites, fn {_, map, openssl} ->
      case format do
        :openssl -> openssl
        _ -> map
      end
    end)
  end

  def filter_cipher_suites(suites, filters) do
    Enum.filter(suites, fn suite ->
      Enum.all?(filters, fn {key, fun} -> fun.(Map.get(suite, key)) end)
    end)
  end

  def suite_to_str(%{} = suite) do
    case Enum.find(@tls13 ++ @tls12, fn {_, map, _} -> map == suite end) do
      {name, _, _} -> String.to_charlist(name)
      nil -> ~c"TLS_NULL_WITH_NULL_NULL"
    end
  end

  def str_to_suite(name) do
    case Enum.find(@tls13 ++ @tls12, fn {n, _, o} -> n == to_string(name) or List.to_string(o) == to_string(name) end) do
      {_, map, _} -> map
      nil -> {:error, {:not_recognized, name}}
    end
  end

  ## Connecting

  def connect(socket, options) when is_reference(socket), do: connect(socket, options, :infinity)

  def connect(socket, options, timeout) when is_reference(socket) do
    with {:ok, options} <- client_options(options) do
      await(:tonic.ssl_upgrade(socket, runtime_options(options), timeout), options)
    end
  end

  def connect(host, port, options), do: connect(host, port, options, :infinity)

  def connect(host, port, options, timeout) do
    with {:ok, options} <- client_options(options) do
      await(:tonic.ssl_connect(host, port, runtime_options(options), timeout), options)
    end
  end

  ## Serving

  def listen(port, options) do
    case server_options(options) do
      {:ok, options} ->
        case :tonic.ssl_listen(port, runtime_options(options)) do
          {:ok, socket} = result ->
            Tonic.SSL.Options.put(socket, options)
            result
          error -> error
        end
      error -> error
    end
  end

  def transport_accept(socket), do: transport_accept(socket, :infinity)
  def transport_accept(socket, timeout) do
    case await(:tonic.ssl_accept(socket, timeout)) do
      {:ok, accepted} = result ->
        Tonic.SSL.Options.put(accepted, Tonic.SSL.Options.get(socket))
        result
      error -> error
    end
  end

  def handshake(socket), do: handshake(socket, [], :infinity)
  def handshake(socket, timeout) when is_integer(timeout) or timeout == :infinity, do: handshake(socket, [], timeout)
  def handshake(socket, options) when is_list(options), do: handshake(socket, options, :infinity)

  def handshake(socket, options, timeout) do
    inherited = Tonic.SSL.Options.get(socket)
    options = Keyword.merge(inherited, normalize(options))
    try do
      await(:tonic.ssl_handshake(socket, runtime_options(options), timeout), options)
    after
      Tonic.SSL.Options.delete(socket)
    end
  end

  ## Data

  def send(socket, data) do
    case :tonic.ssl_send(socket, data) do
      ref when is_reference(ref) -> await(ref)
      result -> result
    end
  end

  def recv(socket, length), do: recv(socket, length, :infinity)
  def recv(socket, length, timeout), do: await(:tonic.ssl_recv(socket, length, timeout))

  def close(socket) do
    Tonic.SSL.Options.delete(socket)
    :tonic.ssl_close(socket)
  end
  def close(socket, _how_or_timeout), do: close(socket)

  def shutdown(socket, how) do
    case :tonic.ssl_shutdown(socket, how) do
      ref when is_reference(ref) -> await(ref)
      result -> result
    end
  end

  ## Options and details

  def setopts(socket, options), do: :tonic.ssl_setopts(socket, options)

  # OTP answers socket options in reverse order of the request
  def getopts(socket, names) do
    with {:ok, found} <- :tonic.ssl_getopts(socket, names), do: {:ok, Enum.reverse(found)}
  end

  def controlling_process(socket, pid) do
    case :tonic.ssl_controlling_process(socket, pid) do
      :ok ->
        Tonic.SSL.Options.transfer(socket, pid)
        :ok
      error -> error
    end
  end
  def peername(socket), do: :tonic.ssl_peername(socket)
  def sockname(socket), do: :tonic.ssl_sockname(socket)
  def getstat(socket), do: :tonic.ssl_getstat(socket)

  def getstat(socket, options) do
    with {:ok, stats} <- getstat(socket), do: {:ok, Enum.filter(stats, fn {k, _} -> k in options end)}
  end

  def negotiated_protocol(socket) do
    case :tonic.ssl_info(socket) do
      {:ok, {_, _, :undefined, _}} -> {:error, :protocol_not_negotiated}
      {:ok, {_, _, protocol, _}} -> {:ok, protocol}
      error -> error
    end
  end

  def connection_information(socket) do
    case :tonic.ssl_info(socket) do
      {:ok, {protocol, suite, _alpn, sni}} ->
        sni = if sni == :undefined, do: [], else: [sni_hostname: String.to_charlist(sni)]

        {:ok,
         [protocol: protocol, session_resumption: false, selected_cipher_suite: suite(suite)] ++
           sni ++ [versions: Enum.map(versions()[:supported], &version_tuple/1)]}

      error ->
        error
    end
  end

  def connection_information(socket, items) do
    with {:ok, info} <- connection_information(socket) do
      {:ok, Enum.filter(info, fn {k, _} -> k in items end)}
    end
  end

  def peercert(socket) do
    case :tonic.ssl_peercerts(socket) do
      {:ok, [der | _]} -> {:ok, der}
      {:ok, []} -> {:error, :no_peercert}
      error -> error
    end
  end

  def format_error({:tls_alert, {_, description}}), do: description
  def format_error(reason), do: ~c"Unexpected error: " ++ :io_lib.format(~c"~p", [reason])

  defp suite(name) do
    case List.keyfind(@tls13 ++ @tls12, name, 0) do
      {_, map, _} -> map
      nil -> %{key_exchange: :any, cipher: :null, mac: :null, prf: :null}
    end
  end

  defp version_tuple(:"tlsv1.3"), do: {3, 4}
  defp version_tuple(:"tlsv1.2"), do: {3, 3}

  defp await(ref) when is_reference(ref) do
    receive do
      {:"$tonic_ssl", ^ref, result} -> result
    end
  end
  defp await(result), do: result

  defp await(ref, options) when is_reference(ref) do
    worker = spawn_link(fn -> callback_loop(options) end)
    try do
      await_callback(ref, worker)
    after
      monitor = Process.monitor(worker)
      Process.unlink(worker)
      Process.exit(worker, :kill)
      receive do
        {:DOWN, ^monitor, :process, ^worker, _} -> :ok
      end
      flush_callback_results(worker)
    end
  end

  defp await(result, _options), do: result

  defp await_callback(ref, worker) do
    receive do
      {:"$tonic_ssl", ^ref, result} -> result
      {:"$tonic_ssl_callback", ^ref, callback, event} ->
        Kernel.send(worker, {:callback, self(), callback, event})
        await_callback(ref, worker)
      {:tonic_ssl_callback_result, ^worker, callback, result} ->
        :tonic.ssl_callback_reply(callback, result)
        await_callback(ref, worker)
    end
  end

  defp flush_callback_results(worker) do
    receive do
      {:tonic_ssl_callback_result, ^worker, _, _} -> flush_callback_results(worker)
    after
      0 -> :ok
    end
  end

  defp callback_loop(options) do
    receive do
      {:callback, caller, reference, event} ->
        {result, options} = callback(event, options)
        Kernel.send(caller, {:tonic_ssl_callback_result, self(), reference, result})
        callback_loop(options)
    end
  end

  defp callback({:verify, der, event}, options) do
    {fun, state} = Keyword.fetch!(Enum.reverse(options), :verify_fun)
    cert = :public_key.pkix_decode_cert(der, :otp)
    state = verify_extensions(cert, fun, state, event)
    case fun.(cert, event, state) do
      {:valid, state} -> {true, Keyword.put(options, :verify_fun, {fun, state})}
      {:unknown, state} -> {event != :valid_peer and not match?({:bad_cert, _}, event), Keyword.put(options, :verify_fun, {fun, state})}
      {:fail, _reason} -> {false, options}
    end
  rescue
    _ -> {false, options}
  catch
    _, _ -> {false, options}
  end

  defp verify_extensions({:OTPCertificate, tbs, _, _} = cert, fun, state, event)
       when event in [:valid, :valid_peer] do
    extensions = elem(tbs, 10)
    Enum.reduce(if(is_list(extensions), do: extensions, else: []), state, fn
      {:Extension, {2, 5, 29, 17} = oid, critical, value}, state ->
        extension = {:Extension, oid, critical, :public_key.hostname_names(value)}
        case fun.(cert, {:extension, extension}, state) do
          {:valid, state} -> state
          {:unknown, state} when not critical -> state
          _ -> throw(:invalid_extension)
        end
      _, state -> state
    end)
  end

  defp verify_extensions(_cert, _fun, state, _event), do: state

  defp callback({:hostname, der, hostname}, options) do
    result = :public_key.pkix_verify_hostname(der, [{:dns_id, String.to_charlist(hostname)}],
      Keyword.get(options, :customize_hostname_check, []))
    {result, options}
  rescue
    _ -> {false, options}
  end

  defp callback({:partial_chain, certificates}, options) do
    {Keyword.fetch!(options, :partial_chain).(certificates), options}
  rescue
    _ -> {:unknown_ca, options}
  catch
    _, _ -> {:unknown_ca, options}
  end

  defp callback({:sni, hostname}, options) do
    case Keyword.fetch!(Enum.reverse(options), :sni_fun).(String.to_charlist(hostname)) do
      :undefined -> {[], options}
      extra when is_list(extra) ->
        extra = normalize(extra)
        {runtime_options(extra), Keyword.merge(options, extra)}
      _ -> {false, options}
    end
  rescue
    _ -> {false, options}
  catch
    _, _ -> {false, options}
  end

  defp runtime_options(options) do
    Enum.flat_map(options, fn
      {:verify_fun, {_fun, _state}} -> [tonic_verify_callback: true]
      {:customize_hostname_check, _checks} -> [tonic_hostname_callback: true]
      {:sni_fun, _fun} -> [tonic_sni_callback: true]
      {:partial_chain, _fun} -> [tonic_partial_chain_callback: true]
      {:ciphers, suites} -> [ciphers: Enum.map(suites, &cipher_name/1)]
      other -> [other]
    end)
  end

  defp cipher_name(%{} = suite), do: suite_to_str(suite) |> to_string()
  defp cipher_name(name) do
    case str_to_suite(name) do
      %{} = suite -> suite_to_str(suite) |> to_string()
      _ -> to_string(name)
    end
  end

  ## Option normalization

  defp client_options(options) do
    options = normalize(options)
    verify = :proplists.get_value(:verify, Enum.reverse(options), :verify_peer)
    trusted? = List.keymember?(options, :cacerts, 0) or List.keymember?(options, :cacertfile, 0)

    cond do
      verify == :verify_peer and not trusted? ->
        {:error, {:options, :incompatible, [verify: :verify_peer, cacerts: :undefined]}}

      true ->
        {:ok, [{:verify, verify} | List.keydelete(options, :verify, 0)]}
    end
  end

  defp server_options(options) do
    options = normalize(options)
    verify = :proplists.get_value(:verify, Enum.reverse(options), :verify_none)
    {:ok, [{:verify, verify} | List.keydelete(options, :verify, 0)]}
  end

  @doc false
  def normalize(options) do
    Enum.flat_map(options, fn
      {:cacerts, certs} -> [{:cacerts, Enum.map(certs, &der/1)}]
      {:cacertfile, :undefined} -> []
      {:cert, cert} when is_list(cert) -> [{:cert, Enum.map(cert, &der/1)}]
      {:cert, cert} -> [{:cert, [der(cert)]}]
      {:key, {_type, key}} when is_binary(key) -> [{:key, key}]
      {:key, key} -> [{:key, key}]
      {:certs_keys, [first | _]} when is_map(first) -> Map.to_list(first) |> normalize()
      {:sni_hosts, hosts} -> [{:sni_hosts, Enum.map(hosts, fn {host, opts} -> {host, normalize(opts)} end)}]
      other -> [other]
    end)
  end

  defp der({:cert, der, _}), do: der
  defp der(der), do: der
end

defmodule Tonic.SSL.Options do
  @moduledoc false
  use GenServer

  def put(socket, options), do: request({:put, socket, options, self()})
  def get(socket), do: request({:get, socket})
  def delete(socket), do: request({:delete, socket})
  def transfer(socket, owner), do: request({:transfer, socket, owner})

  defp request(message) do
    case GenServer.start(__MODULE__, nil, name: __MODULE__) do
      {:ok, _} -> :ok
      {:error, {:already_started, _}} -> :ok
    end
    GenServer.call(__MODULE__, message)
  end

  def init(_), do: {:ok, %{}}

  def handle_call({:get, socket}, _from, state) do
    options = case Map.get(state, socket) do
      {options, _monitor} -> options
      nil -> []
    end
    {:reply, options, state}
  end

  def handle_call({:put, socket, options, owner}, _from, state) do
    state = forget(state, socket)
    {:reply, :ok, Map.put(state, socket, {options, Process.monitor(owner)})}
  end

  def handle_call({:transfer, socket, owner}, _from, state) do
    state = case Map.get(state, socket) do
      {options, _monitor} -> Map.put(forget(state, socket), socket, {options, Process.monitor(owner)})
      nil -> state
    end
    {:reply, :ok, state}
  end

  def handle_call({:delete, socket}, _from, state), do: {:reply, :ok, forget(state, socket)}

  def handle_info({:DOWN, monitor, :process, _pid, _reason}, state) do
    {:noreply, Map.reject(state, fn {_socket, {_options, reference}} -> reference == monitor end)}
  end

  defp forget(state, socket) do
    case Map.pop(state, socket) do
      {{_options, monitor}, state} ->
        Process.demonitor(monitor, [:flush])
        state
      {nil, state} -> state
    end
  end
end
