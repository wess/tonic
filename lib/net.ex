defmodule :gen_tcp do
  # TCP over the runtime's sockets. Waiting operations reply with
  # {:"$tonic_tcp", ref, result}; timeouts are enforced by the runtime.

  def listen(port, options), do: :tonic.tcp_listen(port, options)

  def accept(socket), do: accept(socket, :infinity)
  def accept(socket, timeout), do: await(:tonic.tcp_accept(socket, timeout))

  def connect(address, port, options), do: connect(address, port, options, :infinity)
  def connect(address, port, options, timeout), do: await(:tonic.tcp_connect(address, port, options, timeout))

  def recv(socket, length), do: recv(socket, length, :infinity)
  def recv(socket, length, timeout), do: await(:tonic.tcp_recv(socket, length, timeout))

  def send(socket, data) do
    case :tonic.tcp_send(socket, data) do
      ref when is_reference(ref) -> await(ref)
      result -> result
    end
  end

  def close(socket), do: :tonic.tcp_close(socket)
  def shutdown(socket, how), do: :tonic.tcp_shutdown(socket, how)
  def controlling_process(socket, pid), do: :tonic.tcp_controlling_process(socket, pid)

  defp await(ref) do
    receive do
      {:"$tonic_tcp", ^ref, result} -> result
    end
  end
end

defmodule :inet do
  def setopts(socket, options), do: :tonic.inet_setopts(socket, options)
  def getopts(socket, options), do: :tonic.inet_getopts(socket, options)
  def sockname(socket), do: :tonic.inet_sockname(socket)
  def peername(socket), do: :tonic.inet_peername(socket)
  def getstat(socket), do: :tonic.inet_getstat(socket)

  def getstat(socket, options) do
    with {:ok, stats} <- getstat(socket), do: {:ok, Enum.filter(stats, fn {k, _} -> k in options end)}
  end

  def port(socket) do
    with {:ok, {_, port}} <- sockname(socket), do: {:ok, port}
  end

  def close(socket), do: :gen_tcp.close(socket)

  def ntoa({a, b, c, d}) when a in 0..255 and b in 0..255 and c in 0..255 and d in 0..255,
    do: ~c"#{a}.#{b}.#{c}.#{d}"

  def ntoa({0, 0, 0, 0, 0, 65535, ab, cd}), do: ~c"::ffff:" ++ ntoa({div(ab, 256), rem(ab, 256), div(cd, 256), rem(cd, 256)})

  def ntoa({_, _, _, _, _, _, _, _} = address) do
    groups = address |> Tuple.to_list() |> Enum.map(&String.downcase(Integer.to_string(&1, 16)))
    ipv6_compress(groups) |> String.to_charlist()
  end

  def ntoa(_), do: {:error, :einval}

  defp ipv6_compress(groups) do
    runs =
      groups
      |> Enum.with_index()
      |> Enum.chunk_by(fn {g, _} -> g == "0" end)
      |> Enum.filter(fn [{g, _} | _] = run -> g == "0" and length(run) > 1 end)

    case Enum.max_by(runs, &length/1, fn -> nil end) do
      nil ->
        Enum.join(groups, ":")

      [{_, first} | _] = run ->
        {left, rest} = Enum.split(groups, first)
        right = Enum.drop(rest, length(run))
        Enum.join(left, ":") <> "::" <> Enum.join(right, ":")
    end
  end

  def parse_address(address) when is_list(address) do
    case parse_ipv4_address(address) do
      {:ok, ip} -> {:ok, ip}
      _ -> parse_ipv6_address(address)
    end
  end

  def parse_strict_address(address), do: parse_address(address)

  def parse_ipv4strict_address(address), do: parse_ipv4_address(address)

  def parse_ipv4_address(address) when is_list(address) do
    parts = address |> List.to_string() |> String.split(".")

    with 4 <- length(parts),
         octets when is_list(octets) <- Enum.map(parts, &octet/1),
         false <- Enum.member?(octets, :error) do
      {:ok, List.to_tuple(octets)}
    else
      _ -> {:error, :einval}
    end
  end

  def parse_ipv6_address(address) when is_list(address) do
    string = List.to_string(address)

    {head, tail} =
      case String.split(string, "::") do
        [all] -> {all, nil}
        [head, tail] -> {head, tail}
        _ -> {nil, nil}
      end

    groups = fn
      "" -> []
      part -> String.split(part, ":")
    end

    with true <- head != nil,
         left <- groups.(head),
         right <- if(tail, do: groups.(tail), else: []),
         missing <- 8 - length(left) - length(right),
         true <- (tail == nil and missing == 0) or (tail != nil and missing >= 1),
         values <- Enum.map(left ++ List.duplicate("0", if(tail, do: missing, else: 0)) ++ right, &hex_group/1),
         false <- Enum.member?(values, :error) do
      {:ok, List.to_tuple(values)}
    else
      _ -> {:error, :einval}
    end
  end

  defp octet(part) do
    case Integer.parse(part) do
      {n, ""} when n in 0..255 -> n
      _ -> :error
    end
  end

  defp hex_group(part) do
    case Integer.parse(part, 16) do
      {n, ""} when n in 0..65535 and byte_size(part) <= 4 -> n
      _ -> :error
    end
  end
end
