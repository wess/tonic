defmodule Port do
  def open(name, options) do
    :erlang.open_port(name, options)
  end

  def close(port) do
    :erlang.port_close(port)
  end

  def command(port, data, options \\ []) do
    :erlang.port_command(port, data, options)
  end

  def connect(port, pid) do
    :erlang.port_connect(port, pid)
  end

  def info(port) do
    nilify(:erlang.port_info(port))
  end

  def info(port, spec)

  def info(port, :registered_name) do
    case :erlang.port_info(port, :registered_name) do
      :undefined -> nil
      [] -> {:registered_name, []}
      other -> other
    end
  end

  def info(port, item) do
    nilify(:erlang.port_info(port, item))
  end

  def monitor(port) do
    :erlang.monitor(:port, port)
  end

  defdelegate demonitor(monitor_ref, options \\ []), to: :erlang

  def list do
    :erlang.ports()
  end

  defp nilify(:undefined), do: nil
  defp nilify(other), do: other
end
