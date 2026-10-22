defmodule :gen_udp do
  # UDP over the runtime's sockets. Waiting operations reply with
  # {:"$tonic_udp", ref, result}; timeouts are enforced by the runtime.

  def open(port), do: open(port, [])

  def open(port, options) do
    case :tonic.udp_open(port, options) do
      :badarg -> exit(:badarg)
      result -> result
    end
  end

  def connect(socket, %{addr: address, port: port}), do: connect(socket, address, port)
  def connect(socket, address, port), do: await(:tonic.udp_connect(socket, address, port))

  def send(socket, data) do
    case :tonic.udp_send_connected(socket, data) do
      ref when is_reference(ref) -> await(ref)
      result -> result
    end
  end

  def send(socket, {address, port}, data), do: send(socket, address, port, data)
  def send(socket, %{addr: address, port: port}, data), do: send(socket, address, port, data)

  def send(socket, address, port, data) do
    case :tonic.udp_send(socket, address, port, data) do
      ref when is_reference(ref) -> await(ref)
      result -> result
    end
  end

  def recv(socket, length), do: recv(socket, length, :infinity)
  def recv(socket, length, timeout), do: await(:tonic.udp_recv(socket, length, timeout))

  def close(socket), do: :tonic.udp_close(socket)

  # As OTP does: pause delivery, hand datagrams already in our mailbox to
  # the new owner, then restore the active mode.
  def controlling_process(socket, pid) do
    case :tonic.udp_owner_check(socket, pid) do
      :same ->
        :ok

      :ok ->
        {:ok, [active: active]} = :inet.getopts(socket, [:active])
        :inet.setopts(socket, active: false)
        forward(socket, pid)
        :tonic.udp_controlling_process(socket, pid)
        :inet.setopts(socket, active: active)

      error ->
        error
    end
  end

  defp forward(socket, pid) do
    receive do
      {:udp, ^socket, _, _, _} = message ->
        Kernel.send(pid, message)
        forward(socket, pid)
    after
      0 -> :ok
    end
  end

  defp await(ref) do
    receive do
      {:"$tonic_udp", ^ref, result} -> result
    end
  end
end
