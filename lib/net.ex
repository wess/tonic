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
