defmodule ExUnit.CaptureIO do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.










































































































































  def capture_io(fun) when is_function(fun, 0) do
    {_result, capture} = with_io(fun)
    capture
  end







  def capture_io(device_pid_input_or_options, fun)

  def capture_io(device_or_pid, fun)
      when (is_atom(device_or_pid) or is_pid(device_or_pid)) and is_function(fun, 0) do
    {_result, capture} = with_io(device_or_pid, fun)
    capture
  end

  def capture_io(input_or_options, fun)
      when (is_binary(input_or_options) or is_list(input_or_options)) and is_function(fun, 0) do
    {_result, capture} = with_io(input_or_options, fun)
    capture
  end







  def capture_io(device_or_pid, input_or_options, fun)
      when (is_atom(device_or_pid) or is_pid(device_or_pid)) and
             (is_binary(input_or_options) or is_list(input_or_options)) and is_function(fun, 0) do
    {_result, capture} = with_io(device_or_pid, input_or_options, fun)
    capture
  end




















  def with_io(fun) when is_function(fun, 0) do
    with_io(:stdio, [], fun)
  end








  def with_io(device_pid_input_or_options, fun)

  def with_io(device, fun) when is_atom(device) and is_function(fun, 0) do
    with_io(device, [], fun)
  end

  def with_io(pid, fun) when is_pid(pid) and is_function(fun, 0) do
    with_io(pid, [], fun)
  end

  def with_io(input, fun) when is_binary(input) and is_function(fun, 0) do
    with_io(:stdio, [input: input], fun)
  end

  def with_io(options, fun) when is_list(options) and is_function(fun, 0) do
    with_io(:stdio, options, fun)
  end








  def with_io(device_or_pid, input_or_options, fun)

  def with_io(device, input, fun)
      when is_atom(device) and is_binary(input) and is_function(fun, 0) do
    do_with_io(map_dev(device), [input: input], fun)
  end

  def with_io(device, options, fun)
      when is_atom(device) and is_list(options) and is_function(fun, 0) do
    do_with_io(map_dev(device), options, fun)
  end

  def with_io(pid, input, fun)
      when is_pid(pid) and is_binary(input) and is_function(fun, 0) do
    do_with_io(pid, [input: input], fun)
  end

  def with_io(pid, options, fun)
      when is_pid(pid) and is_list(options) and is_function(fun, 0) do
    do_with_io(pid, options, fun)
  end

  defp map_dev(:standard_io), do: self()
  defp map_dev(:stdio), do: self()
  defp map_dev(:stderr), do: :standard_error
  defp map_dev(other), do: other

  defp do_with_io(pid, options, fun) when is_pid(pid) do
    prompt_config = Keyword.get(options, :capture_prompt, true)
    encoding = Keyword.get(options, :encoding, :unicode)
    input = Keyword.get(options, :input, "")

    {:group_leader, original_gl} =
      Process.info(pid, :group_leader) || {:group_leader, Process.group_leader()}

    {:ok, capture_gl} = StringIO.open(input, capture_prompt: prompt_config, encoding: encoding)

    try do
      Process.group_leader(pid, capture_gl)
      do_capture_gl(capture_gl, fun)
    after
      Process.group_leader(pid, original_gl)
    end
  end

  defp do_with_io(device, options, fun) when is_atom(device) do
    input = Keyword.get(options, :input, "")
    encoding = Keyword.get(options, :encoding, :unicode)

    case ExUnit.CaptureServer.device_capture_on(device, encoding, input) do
      {:ok, ref} ->
        try do
          result = fun.()
          {result, ExUnit.CaptureServer.device_output(device, ref)}
        after
          ExUnit.CaptureServer.device_capture_off(ref)
        end

      {:error, :no_device} ->
        raise "could not find IO device registered at #{inspect(device)}"

      {:error, {:changed_encoding, current_encoding}} ->
        raise ArgumentError, """
        attempted to change the encoding for a currently captured device #{inspect(device)}.

        Currently set as: #{inspect(current_encoding)}
        Given: #{inspect(encoding)}

        If you need to use multiple encodings on a captured device, you cannot \
        run your test asynchronously
        """

      {:error, :input_on_already_captured_device} ->
        raise ArgumentError,
              "attempted multiple captures on device #{inspect(device)} with input. " <>
                "If you need to give an input to a captured device, you cannot run your test asynchronously"
    end
  end

  defp do_capture_gl(string_io, fun) do
    try do
      fun.()
    catch
      kind, reason ->
        _ = StringIO.close(string_io)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      result ->
        {:ok, {_input, output}} = StringIO.close(string_io)
        {result, output}
    end
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../ex_unit/ex_unit/capture_io.ex (docs and specs stripped;
# line numbers match the original).
