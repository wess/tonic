defmodule Process do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.































































































  defdelegate alive?(pid), to: :erlang, as: :is_process_alive







  defdelegate get(), to: :erlang

















  def get(key, default \\ nil) do
    case :erlang.get(key) do
      :undefined -> default
      value -> value
    end
  end


















  defdelegate get_keys(), to: :erlang







  defdelegate get_keys(value), to: :erlang

















  def put(key, value) do
    nilify(:erlang.put(key, value))
  end

















  def delete(key) do
    nilify(:erlang.erase(key))
  end










































  defdelegate exit(pid, reason), to: :erlang


















































































  # Max value for a receive's after clause
  @max_receive_after 0xFFFFFFFF


  def sleep(timeout) when is_integer(timeout) and timeout > @max_receive_after do
    receive after: (@max_receive_after -> sleep(timeout - @max_receive_after))
  end

  def sleep(timeout)
      when is_integer(timeout) and timeout >= 0
      when timeout == :infinity do
    receive after: (timeout -> :ok)
  end































  defdelegate send(dest, msg, options), to: :erlang





































  def send_after(dest, msg, time, opts \\ []) do
    :erlang.send_after(time, dest, msg, opts)
  end




































  defdelegate cancel_timer(timer_ref, options \\ []), to: :erlang

















  defdelegate read_timer(timer_ref), to: :erlang



































  defdelegate spawn(fun, opts), to: :erlang, as: :spawn_opt
















  defdelegate spawn(mod, fun, args, opts), to: :erlang, as: :spawn_opt







































  def monitor(item) do
    :erlang.monitor(:process, item)
  end








































  def monitor(item, options) do
    :erlang.monitor(:process, item, options)
  end





















  defdelegate demonitor(monitor_ref, options \\ []), to: :erlang




















  defdelegate list(), to: :erlang, as: :processes
























  defdelegate link(pid_or_port), to: :erlang















  defdelegate unlink(pid_or_port), to: :erlang
































  def register(pid_or_port, name)
      when is_atom(name) and name not in [nil, false, true, :undefined] do
    :erlang.register(name, pid_or_port)
  catch
    :error, :badarg when node(pid_or_port) != node() ->
      message = "could not register #{inspect(pid_or_port)} because it belongs to another node"
      :erlang.error(ArgumentError.exception(message), [pid_or_port, name])

    :error, :badarg ->
      message =
        "could not register #{inspect(pid_or_port)} with " <>
          "name #{inspect(name)} because it is not alive, the name is already " <>
          "taken, or it has already been given another name"

      :erlang.error(ArgumentError.exception(message), [pid_or_port, name])
  end





















  defdelegate unregister(name), to: :erlang

















  def whereis(name) do
    nilify(:erlang.whereis(name))
  end













  defdelegate group_leader(), to: :erlang










  def group_leader(pid, leader) do
    :erlang.group_leader(leader, pid)
  end














  defdelegate registered(), to: :erlang


















  # :off_heap | :on_heap twice because :erlang.message_queue_data() is not exported







  defdelegate flag(flag, value), to: :erlang, as: :process_flag
















  defdelegate flag(pid, flag, value), to: :erlang, as: :process_flag










  def info(pid) do
    nilify(:erlang.process_info(pid))
  end












  def info(pid, spec)

  def info(pid, :registered_name) do
    case :erlang.process_info(pid, :registered_name) do
      :undefined -> nil
      [] -> {:registered_name, []}
      other -> other
    end
  end

  def info(pid, spec) do
    nilify(:erlang.process_info(pid, spec))
  end














  defdelegate hibernate(mod, fun_name, args), to: :erlang

























  defdelegate alias(), to: :erlang
















  defdelegate alias(options), to: :erlang





















  defdelegate unalias(alias), to: :erlang



















  def set_label(label) do
    # TODO: switch to `:proc_lib.set_label/2` when we require Erlang/OTP 27+
    Process.put(:"$process_label", label)
    # mimic return value of `:proc_lib.set_label/2`
    :ok
  end


  defp nilify(:undefined), do: nil
  defp nilify(other), do: other
end

# Imported from Elixir 1.18.3 lib/elixir/lib/process.ex (docs and specs stripped;
# line numbers match the original).
