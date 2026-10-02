defmodule Kernel do
  def to_timeout(:infinity), do: :infinity
  def to_timeout(timeout) when is_integer(timeout) and timeout >= 0, do: timeout

  def to_timeout(%{__struct__: Duration} = duration) do
    case duration do
      %{year: year} when year != 0 ->
        raise ArgumentError, "duration with a non-zero year cannot be reliably converted to timeouts"

      %{month: month} when month != 0 ->
        raise ArgumentError, "duration with a non-zero month cannot be reliably converted to timeouts"

      _other ->
        {microsecond, _precision} = duration.microsecond
        millisecond = :erlang.convert_time_unit(microsecond, :microsecond, :millisecond)

        duration.week * 604_800_000 + duration.day * 86_400_000 + duration.hour * 3_600_000 +
          duration.minute * 60_000 + duration.second * 1000 + millisecond
    end
  end

  def to_timeout(components) when is_list(components) do
    reducer = fn
      {key, value}, {acc, seen_keys} when is_integer(value) and value >= 0 ->
        if :lists.member(key, seen_keys) do
          raise ArgumentError, "timeout component #{inspect(key)} is duplicated"
        end

        factor =
          case key do
            :week -> 604_800_000
            :day -> 86_400_000
            :hour -> 3_600_000
            :minute -> 60_000
            :second -> 1000
            :millisecond -> 1
            other ->
              raise ArgumentError,
                    "timeout component #{inspect(other)} is not a valid timeout component, valid " <>
                      "values are: :week, :day, :hour, :minute, :second, :millisecond"
          end

        {acc + value * factor, [key | seen_keys]}

      {key, value}, {_acc, _seen_keys} ->
        raise ArgumentError,
              "timeout component #{inspect(key)} must be a non-negative integer, got: #{inspect(value)}"
    end

    elem(:lists.foldl(reducer, {0, []}, components), 0)
  end

  def binary_slice(binary, start, size)
      when is_binary(binary) and is_integer(start) and is_integer(size) and size >= 0 do
    total = byte_size(binary)
    start = if start < 0, do: max(total + start, 0), else: start

    case start < total do
      true -> :erlang.binary_part(binary, start, min(size, total - start))
      false -> ""
    end
  end

  def binary_slice(binary, first..last//step) when is_binary(binary) and step > 0 do
    total = byte_size(binary)
    first = if first < 0, do: max(first + total, 0), else: first
    last = if last < 0, do: last + total, else: last
    amount = last - first + 1

    if first < total and amount > 0 do
      part = binary_part(binary, first, min(amount, total - first))

      if step == 1 do
        part
      else
        for i <- 0..(byte_size(part) - 1)//step, into: "", do: <<:binary.at(part, i)>>
      end
    else
      ""
    end
  end

  def binary_slice(binary, _.._//_ = range) when is_binary(binary) do
    raise ArgumentError,
          "binary_slice/2 does not accept ranges with negative steps, got: #{inspect(range)}"
  end

  def inspect(term, opts \\ []) when is_list(opts) do
    opts = Inspect.Opts.new(opts)

    limit =
      case opts.pretty do
        true -> opts.width
        false -> :infinity
      end

    doc = Inspect.Algebra.group(Inspect.Algebra.to_doc(term, opts))
    IO.iodata_to_binary(Inspect.Algebra.format(doc, limit))
  end

  def to_string(term) when is_binary(term), do: term
  def to_string(term), do: String.Chars.to_string(term)

  def to_charlist(term), do: List.Chars.to_charlist(term)

  def raise(x), do: :erlang.error(__exception__(x))
  def raise(module, attrs), do: :erlang.error(__exception__(module, attrs))
  def reraise(x, stacktrace), do: :erlang.raise(:error, __exception__(x), stacktrace)
  def reraise(module, attrs, stacktrace), do: :erlang.raise(:error, __exception__(module, attrs), stacktrace)

  @doc false
  def __exception__(msg) when is_binary(msg), do: RuntimeError.exception(msg)
  def __exception__(%{__exception__: true} = exception), do: exception
  def __exception__(module) when is_atom(module), do: module.exception([])

  def __exception__(other) do
    ArgumentError.exception(
      "raise/1 and reraise/2 expect a module name, string or exception as the first argument, got: #{inspect(other)}"
    )
  end

  @doc false
  def __exception__(module, attrs) when is_atom(module), do: module.exception(attrs)

  def then(value, fun), do: fun.(value)

  def tap(value, fun) do
    _ = fun.(value)
    value
  end

  def dbg(value) do
    IO.puts(inspect(value, pretty: true))
    value
  end

  def left =~ right when is_binary(left) and is_binary(right), do: String.contains?(left, right)
  def left =~ right when is_binary(left), do: Regex.match?(right, left)

  def base ** exponent, do: :tonic.pow(base, exponent)

  def struct(struct, fields \\ []) do
    struct(struct, fields, fn
      {:__struct__, _val}, acc ->
        acc

      {key, val}, acc ->
        case acc do
          %{^key => _} -> %{acc | key => val}
          _ -> acc
        end
    end)
  end

  def struct!(struct, fields \\ [])

  def struct!(struct, fields) when is_atom(struct) do
    validate_struct!(struct.__struct__(fields), struct, 1)
  end

  def struct!(struct, fields) when is_map(struct) do
    struct(struct, fields, fn
      {:__struct__, _}, acc ->
        acc

      {key, val}, acc ->
        Map.replace!(acc, key, val)
    end)
  end

  defp struct(struct, [], _fun) when is_atom(struct) do
    validate_struct!(struct.__struct__(), struct, 0)
  end

  defp struct(struct, fields, fun) when is_atom(struct) do
    struct(validate_struct!(struct.__struct__(), struct, 0), fields, fun)
  end

  defp struct(%_{} = struct, [], _fun) do
    struct
  end

  defp struct(%_{} = struct, fields, fun) do
    Enum.reduce(fields, struct, fun)
  end

  defp validate_struct!(%{__struct__: module} = struct, module, _arity) do
    struct
  end

  defp validate_struct!(%{__struct__: struct_name}, module, arity) when is_atom(struct_name) do
    error_message =
      "expected struct name returned by #{inspect(module)}.__struct__/#{arity} to be " <>
        "#{inspect(module)}, got: #{inspect(struct_name)}"

    :erlang.error(ArgumentError.exception(error_message))
  end

  defp validate_struct!(expr, module, arity) do
    error_message =
      "expected #{inspect(module)}.__struct__/#{arity} to return a map with a :__struct__ " <>
        "key that holds the name of the struct (atom), got: #{inspect(expr)}"

    :erlang.error(ArgumentError.exception(error_message))
  end

  def get_in(data, keys)
  def get_in(nil, [_ | _]), do: nil
  def get_in(data, [h]) when is_function(h), do: h.(:get, data, fn x -> x end)
  def get_in(data, [h | t]) when is_function(h), do: h.(:get, data, &get_in(&1, t))
  def get_in(data, [h]), do: Access.get(data, h)
  def get_in(data, [h | t]), do: get_in(Access.get(data, h), t)

  def put_in(data, [_ | _] = keys, value) do
    elem(get_and_update_in(data, keys, fn _ -> {nil, value} end), 1)
  end

  def update_in(data, [_ | _] = keys, fun) when is_function(fun) do
    elem(get_and_update_in(data, keys, fn x -> {nil, fun.(x)} end), 1)
  end

  def get_and_update_in(data, [head], fun) when is_function(head, 3),
    do: head.(:get_and_update, data, fun)

  def get_and_update_in(data, [head | tail], fun) when is_function(head, 3),
    do: head.(:get_and_update, data, &get_and_update_in(&1, tail, fun))

  def get_and_update_in(data, [head], fun) when is_function(fun, 1),
    do: Access.get_and_update(data, head, fun)

  def get_and_update_in(data, [head | tail], fun) when is_function(fun, 1),
    do: Access.get_and_update(data, head, &get_and_update_in(&1, tail, fun))

  def pop_in(nil, [key | _]) do
    raise ArgumentError, "could not pop key #{inspect(key)} on a nil value"
  end

  def pop_in(data, [_ | _] = keys), do: pop_in_data(data, keys)

  defp pop_in_data(nil, [_ | _]), do: :pop
  defp pop_in_data(data, [fun]) when is_function(fun), do: fun.(:get_and_update, data, fn _ -> :pop end)
  defp pop_in_data(data, [fun | tail]) when is_function(fun), do: fun.(:get_and_update, data, &pop_in_data(&1, tail))
  defp pop_in_data(data, [key]), do: Access.pop(data, key)
  defp pop_in_data(data, [key | tail]), do: Access.get_and_update(data, key, &pop_in_data(&1, tail))

  def function_exported?(module, fun, arity), do: :erlang.function_exported(module, fun, arity)
  def macro_exported?(module, fun, arity) when is_atom(module) and is_atom(fun) and is_integer(arity), do: Tonic.Internal.macro_exported?(module, fun, arity)

  def binding, do: []

  def spawn(module, fun, args), do: :erlang.spawn(module, fun, args)
  def spawn_link(module, fun, args), do: :erlang.spawn_link(module, fun, args)
  def spawn_monitor(module, fun, args), do: :erlang.spawn_monitor(module, fun, args)
end

defmodule Tonic.Internal do
  @moduledoc false

  def format_exit({kind, stack, main?}, reason) do
    stack =
      if main? do
        stack ++ [{Code, :require_file, 2, [file: ~c"lib/code.ex", line: 1525]}]
      else
        stack
      end

    {reason, stack} =
      if main?, do: Exception.blame(kind, reason, stack), else: {Exception.normalize(kind, reason, stack), stack}
    banner = format_exit(kind, reason)

    case stack do
      [] -> banner
      _ -> banner <> "\n" <> String.trim_trailing(Exception.format_stacktrace(stack), "\n")
    end
  end

  def format_exit(:error, reason) do
    e = normalize_error(reason)
    "** (" <> inspect(e.__struct__) <> ") " <> Exception.message(e)
  end

  def format_exit(:throw, reason), do: "** (throw) " <> inspect(reason)
  def format_exit(:exit, reason), do: "** (exit) " <> Exception.format_exit(reason)
  def format_exit(kind, reason), do: "** (#{inspect(kind)}) " <> inspect(reason)

  def normalize_error(reason), do: Exception.normalize(:error, reason, [])

  def destructure(nil, n), do: List.duplicate(nil, n)
  def destructure(list, n) when is_list(list), do: do_destructure(list, n)

  defp do_destructure(_, 0), do: []
  defp do_destructure([h | t], n), do: [h | do_destructure(t, n - 1)]
  defp do_destructure([], n), do: List.duplicate(nil, n)

  def bin_chunks(bin, splitter), do: bin_chunks(bin, splitter, [])

  defp bin_chunks(bin, splitter, acc) do
    case splitter.(bin) do
      nil ->
        :lists.reverse(acc)

      rest ->
        n = bit_size(bin) - bit_size(rest)
        <<elem::bitstring-size(n), _::bitstring>> = bin
        bin_chunks(rest, splitter, [elem | acc])
    end
  end

  def remote_name(atom), do: :tonic.inspect_atom(atom, 2)

  @doc false
  # Inspect.Any for structs: the fields to show, honouring @derive Inspect
  # options (only/except/optional), as Inspect.__deriving__ would.
  def inspect_infos(module, struct, dunder, fields) do
    infos = for %{field: f} = info <- fields, f not in [:__struct__, :__exception__], do: info

    if function_exported?(module, :__inspect_derive__, 0) do
      options = module.__inspect_derive__()
      all = Enum.sort(Enum.map(infos, & &1.field))
      only = Keyword.get(options, :only, all)
      except = Keyword.get(options, :except, [])
      optional = Keyword.get(options, :optional, [])
      mod = if all == Enum.sort(only) and except == [], do: Inspect.Map, else: Inspect.Any

      filtered =
        for %{field: f} = info <- infos,
            f in only,
            f not in except,
            not (f in optional and Map.get(dunder, f) == Map.get(struct, f)),
            do: info

      {mod, filtered}
    else
      {Inspect.Map, infos}
    end
  end

  @doc false
  def dbg_header(env) do
    env = Map.update!(env, :file, &(&1 && Path.relative_to_cwd(&1)))
    [stacktrace_entry] = Macro.Env.stacktrace(env)
    "[" <> Exception.format_stacktrace_entry(stacktrace_entry) <> "]"
  end

  @doc false
  def __boot__ do
    pid = :erlang.spawn(Tonic.FileIO, :standard_error_loop, [])
    :erlang.register(:standard_error, pid)
    :ok
  end

  @doc false
  def __run_at_exit__(status) do
    case Application.get_env(:elixir, :"$at_exit", []) do
      [] ->
        if status != 0, do: System.halt(status), else: :ok

      funs ->
        Application.put_env(:elixir, :"$at_exit", [])
        status = Enum.reduce(funs, status, &exec_at_exit/2)
        __run_at_exit__(status)
    end
  end

  # As Kernel.CLI: each hook runs in its own process; exit({:shutdown, n})
  # sets the exit status.
  defp exec_at_exit(fun, status) do
    parent = self()

    {pid, ref} =
      spawn_monitor(fn ->
        try do
          fun.(status)
        catch
          :exit, {:shutdown, int} when is_integer(int) ->
            send(parent, {self(), {:shutdown, int}})
            exit({:shutdown, int})

          :exit, reason
          when reason in [:normal, :shutdown] or
                 (is_tuple(reason) and tuple_size(reason) == 2 and elem(reason, 0) == :shutdown) ->
            send(parent, {self(), {:shutdown, 0}})
            exit(reason)

          kind, reason ->
            IO.write(:stderr, format_exit({kind, __STACKTRACE__, false}, reason) <> "\n")
            send(parent, {self(), {:shutdown, 1}})
            exit(:normal)
        else
          _ -> send(parent, {self(), status})
        end
      end)

    receive do
      {^pid, {:shutdown, int}} ->
        receive do
          {:DOWN, ^ref, _, _, _} -> int
        end

      {^pid, res} ->
        receive do
          {:DOWN, ^ref, _, _, _} -> res
        end

      {:DOWN, ^ref, _, _, _} ->
        1
    end
  end

  def protocol_undefined(protocol, value) do
    raise Protocol.UndefinedError, protocol: protocol, value: value
  end
end
