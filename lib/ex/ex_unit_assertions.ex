defmodule ExUnit.AssertionError do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.





  @no_value :ex_unit_no_meaningful_value












  defexception left: @no_value,
               right: @no_value,
               message: @no_value,
               expr: @no_value,
               args: @no_value,
               doctest: @no_value,
               context: :==





  def no_value do
    @no_value
  end


  def message(exception) do
    "\n\n" <> ExUnit.Formatter.format_assertion_error(exception)
  end
end

defmodule ExUnit.MultiError do









  defexception errors: []


  def message(%{errors: errors}) do
    "got the following errors:\n\n" <>
      Enum.map_join(errors, "\n\n", fn {kind, error, stack} ->
        Exception.format_banner(kind, error, stack)
      end)
  end
end

defmodule ExUnit.Assertions do


























































































































































































  @operator [:==, :<, :>, :<=, :>=, :===, :=~, :!==, :!=, :in]












































































  def __equal__?(left, right) do
    left === right
  end




































































































  def assert(value, message) when is_binary(message) do
    assert(value, message: message)
  end

  def assert(value, opts) when is_list(opts) do
    if !value, do: raise(ExUnit.AssertionError, opts)
    true
  end































































































































  @indent "\n  "
  @max_mailbox_length 10


  def __timeout__(timeout, _) when is_integer(timeout) and timeout >= 0, do: timeout

  def __timeout__(nil, key), do: Application.fetch_env!(:ex_unit, key)

  def __timeout__(timeout, _),
    do: raise(ArgumentError, "timeout must be a non-negative integer, got: #{inspect(timeout)}")


  def __timeout__(pattern, code, pins, pattern_finder, timeout) do
    {:messages, messages} = Process.info(self(), :messages)

    if Enum.any?(messages, pattern_finder) do
      raise ExUnit.AssertionError,
        expr: code,
        message: """
        Found message matching #{Macro.to_string(pattern)} after #{timeout}ms.

        This means the message was delivered too close to the timeout value, you may want to either:

          1. Give an increased timeout to `assert_receive/2`
          2. Increase the default timeout to all `assert_receive` in your
             test_helper.exs by setting ExUnit.configure(assert_receive_timeout: ...)
        """
    else
      {message, mailbox} = format_mailbox(messages)

      # The error contains a special `context` that will be transformed
      # into `{:match, pins}` by the formatter to execute a diff for each
      # message in the `mailbox`
      raise ExUnit.AssertionError,
        left: pattern,
        expr: code,
        message:
          "Assertion failed, no matching message after #{timeout}ms" <>
            ExUnit.Assertions.__pins__(pins) <> message,
        context: {:mailbox, pins, mailbox}
    end
  end


  def __pins__(pins) do
    pins
    |> Enum.filter(fn {{_, ctx}, _} -> ctx == nil end)
    |> Enum.reverse()
    |> Enum.map_join(@indent, fn {{name, _}, var} -> "#{name} = #{inspect(var)}" end)
    |> case do
      "" ->
        ""

      pinned ->
        "\nThe following variables were pinned:" <> @indent <> pinned
    end
  end

  defp format_mailbox(messages) do
    length = length(messages)
    mailbox = Enum.take(messages, -@max_mailbox_length)

    {mailbox_message(length), mailbox}
  end

  defp mailbox_message(0), do: "\nThe process mailbox is empty."

  defp mailbox_message(1) do
    "\nShowing 1 of 1 message in the mailbox"
  end

  defp mailbox_message(length) when length > @max_mailbox_length do
    "\nShowing #{@max_mailbox_length} of #{length} messages in the mailbox"
  end

  defp mailbox_message(length) do
    "\nShowing #{length} of #{length} messages in the mailbox"
  end

























































































































































  def assert_raise(exception, message, function) when is_function(function) do
    error = assert_raise(exception, function)

    match? =
      cond do
        is_binary(message) -> Exception.message(error) == message
        is_struct(message, Regex) -> Exception.message(error) =~ message
      end

    message =
      "Wrong message for #{inspect(exception)}\n" <>
        "expected:\n  #{inspect(message)}\n" <>
        "actual:\n" <> "  #{inspect(Exception.message(error))}"

    if not match?, do: flunk(message)

    error
  end
















  def assert_raise(exception, function) when is_function(function) do
    try do
      function.()
    rescue
      error ->
        name = error.__struct__

        cond do
          name == exception ->
            check_error_message(name, error)
            error

          name == ExUnit.AssertionError ->
            reraise(error, __STACKTRACE__)

          true ->
            message =
              "Expected exception #{inspect(exception)} " <>
                "but got #{inspect(name)} (#{Exception.message(error)})"

            reraise ExUnit.AssertionError, [message: message], __STACKTRACE__
        end
    else
      _ -> flunk("Expected exception #{inspect(exception)} but nothing was raised")
    end
  end

  defp check_error_message(module, error) do
    module.message(error)
  catch
    kind, reason ->
      message =
        "Got exception #{inspect(module)} but it failed to produce a message with:\n\n" <>
          Exception.format(kind, reason, __STACKTRACE__)

      flunk(message)
  end














  def assert_in_delta(value1, value2, delta, message \\ nil)

  def assert_in_delta(_, _, delta, _) when delta < 0 do
    raise ArgumentError, "delta must always be a positive number, got: #{inspect(delta)}"
  end

  def assert_in_delta(value1, value2, delta, message) do
    diff = abs(value1 - value2)

    message =
      message ||
        "Expected the difference between #{inspect(value1)} and " <>
          "#{inspect(value2)} (#{inspect(diff)}) to be less than or equal to #{inspect(delta)}"

    assert diff <= delta, message
  end








































































  def refute(value, message) do
    not assert(!value, message)
  end


























































































  def refute_in_delta(value1, value2, delta, message \\ nil) do
    diff = abs(value1 - value2)

    message =
      if message do
        message <>
          " (difference between #{inspect(value1)} " <>
          "and #{inspect(value2)} is less than #{inspect(delta)})"
      else
        "Expected the difference between #{inspect(value1)} and " <>
          "#{inspect(value2)} (#{inspect(diff)}) to be more than #{inspect(delta)}"
      end

    refute diff < delta, message
  end











  def flunk(message \\ "Flunked!") when is_binary(message) do
    assert false, message: message
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../ex_unit/ex_unit/assertions.ex (docs and specs stripped;
# line numbers match the original).
