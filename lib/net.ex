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
  def getaddrs(host, family) when family in [:inet, :inet6] do
    ref = :tonic.inet_getaddrs(host, family)

    receive do
      {:"$tonic_tcp", ^ref, result} -> result
    end
  end

  def getaddr(host, family) do
    with {:ok, [ip | _]} <- getaddrs(host, family), do: {:ok, ip}
  end

  def gethostbyname(name), do: gethostbyname(name, :inet)

  def gethostbyname(name, family) do
    with {:ok, ips} <- getaddrs(name, family) do
      size = if family == :inet6, do: 16, else: 4
      {:ok, {:hostent, host_chars(name), [], family, size, ips}}
    end
  end

  defp host_chars(name) when is_atom(name), do: Atom.to_charlist(name)
  defp host_chars(name) when is_binary(name), do: String.to_charlist(name)
  defp host_chars(name) when is_list(name), do: name
  defp host_chars(name) when is_tuple(name), do: ntoa(name)

  # the short name: OTP drops the domain
  def gethostname do
    {:ok, :tonic.inet_gethostname() |> String.to_charlist() |> Enum.take_while(&(&1 != ?.))}
  end

  def is_ip_address(address), do: is_ipv4_address(address) or is_ipv6_address(address)

  def is_ipv4_address({a, b, c, d} = ip) when is_integer(a) and is_integer(b) and is_integer(c) and is_integer(d),
    do: Enum.all?(Tuple.to_list(ip), &(&1 in 0..255))

  def is_ipv4_address(_), do: false

  def is_ipv6_address(ip) when is_tuple(ip) and tuple_size(ip) == 8,
    do: Enum.all?(Tuple.to_list(ip), &(is_integer(&1) and &1 in 0..65535))

  def is_ipv6_address(_), do: false

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

  # Same delegations as OTP's inet.erl; the parsers are the bundled inet_parse.
  def ntoa(address), do: :inet_parse.ntoa(address)
  def parse_address(address), do: :inet_parse.address(address)
  def parse_address(address, :inet), do: :inet_parse.ipv4_address(address)
  def parse_address(address, :inet6), do: :inet_parse.ipv6_address(address)
  def parse_strict_address(address), do: :inet_parse.strict_address(address)
  def parse_strict_address(address, :inet), do: :inet_parse.ipv4strict_address(address)
  def parse_strict_address(address, :inet6), do: :inet_parse.ipv6strict_address(address)
  def parse_ipv4_address(address), do: :inet_parse.ipv4_address(address)
  def parse_ipv6_address(address), do: :inet_parse.ipv6_address(address)
  def parse_ipv4strict_address(address), do: :inet_parse.ipv4strict_address(address)
  def parse_ipv6strict_address(address), do: :inet_parse.ipv6strict_address(address)
end
