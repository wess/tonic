defmodule Access do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.






































































































































































































  defmacrop raise_undefined_behaviour(exception, module, top) do
    quote do
      exception =
        case __STACKTRACE__ do
          [unquote(top) | _] ->
            reason =
              """
              #{inspect(unquote(module))} does not implement the Access behaviour

              You can use the "struct.field" syntax to access struct fields. \
              You can also use Access.key!/1 to access struct fields dynamically \
              inside get_in/put_in/update_in\
              """

            %{unquote(exception) | reason: reason}

          _ ->
            unquote(exception)
        end

      reraise exception, __STACKTRACE__
    end
  end



















  def fetch(container, key)

  def fetch(%module{} = container, key) do
    module.fetch(container, key)
  rescue
    exception in UndefinedFunctionError ->
      raise_undefined_behaviour(exception, module, {^module, :fetch, [^container, ^key], _})
  end

  def fetch(map, key) when is_map(map) do
    case map do
      %{^key => value} -> {:ok, value}
      _ -> :error
    end
  end

  def fetch(list, key) when is_list(list) and is_atom(key) do
    case :lists.keyfind(key, 1, list) do
      {_, value} -> {:ok, value}
      false -> :error
    end
  end

  def fetch(list, key) when is_list(list) do
    raise ArgumentError,
          "the Access calls for keywords expect the key to be an atom, got: " <> inspect(key)
  end

  def fetch(nil, _key) do
    :error
  end













  def fetch!(container, key) do
    case fetch(container, key) do
      {:ok, value} -> value
      :error -> raise(KeyError, key: key, term: container)
    end
  end





















  def get(container, key, default \\ nil)

  # Reimplementing the same logic as Access.fetch/2 here is done for performance, since
  # this is called a lot and calling fetch/2 means introducing some overhead (like
  # building the "{:ok, _}" tuple and deconstructing it back right away).

  def get(%module{} = container, key, default) do
    try do
      module.fetch(container, key)
    rescue
      exception in UndefinedFunctionError ->
        raise_undefined_behaviour(exception, module, {^module, :fetch, [^container, ^key], _})
    else
      {:ok, value} -> value
      :error -> default
    end
  end

  def get(map, key, default) when is_map(map) do
    case map do
      %{^key => value} -> value
      _ -> default
    end
  end

  def get(list, key, default) when is_list(list) and is_atom(key) do
    case :lists.keyfind(key, 1, list) do
      {_, value} -> value
      false -> default
    end
  end

  def get(list, key, _default) when is_list(list) and is_integer(key) do
    raise ArgumentError, """
    the Access module does not support accessing lists by index, got: #{inspect(key)}

    Accessing a list by index is typically discouraged in Elixir, \
    instead we prefer to use the Enum module to manipulate lists \
    as a whole. If you really must access a list element by index, \
    you can use Enum.at/2 or the functions in the List module\
    """
  end

  def get(list, key, _default) when is_list(list) do
    raise ArgumentError, """
    the Access module supports only keyword lists (with atom keys), got: #{inspect(key)}

    If you want to search lists of tuples, use List.keyfind/3\
    """
  end

  def get(nil, _key, default) do
    default
  end


























  def get_and_update(container, key, fun)

  def get_and_update(%module{} = container, key, fun) do
    module.get_and_update(container, key, fun)
  rescue
    exception in UndefinedFunctionError ->
      raise_undefined_behaviour(
        exception,
        module,
        {^module, :get_and_update, [^container, ^key, ^fun], _}
      )
  end

  def get_and_update(map, key, fun) when is_map(map) do
    Map.get_and_update(map, key, fun)
  end

  def get_and_update(list, key, fun) when is_list(list) and is_atom(key) do
    Keyword.get_and_update(list, key, fun)
  end

  def get_and_update(list, key, _fun) when is_list(list) and is_integer(key) do
    raise ArgumentError, """
    the Access module does not support accessing lists by index, got: #{inspect(key)}

    Accessing a list by index is typically discouraged in Elixir, \
    instead we prefer to use the Enum module to manipulate lists \
    as a whole. If you really must modify a list element by index, \
    you can use Access.at/1 or the functions in the List module\
    """
  end

  def get_and_update(list, key, _fun) when is_list(list) do
    raise ArgumentError,
          "the Access module supports only keyword lists (with atom keys), got: " <> inspect(key)
  end

  def get_and_update(nil, key, _fun) do
    raise ArgumentError, "could not put/update key #{inspect(key)} on a nil value"
  end




























  def pop(%module{} = container, key) do
    module.pop(container, key)
  rescue
    exception in UndefinedFunctionError ->
      raise_undefined_behaviour(exception, module, {^module, :pop, [^container, ^key], _})
  end

  def pop(map, key) when is_map(map) do
    Map.pop(map, key)
  end

  def pop(list, key) when is_list(list) do
    Keyword.pop(list, key)
  end

  def pop(nil, key) do
    raise ArgumentError, "could not pop key #{inspect(key)} on a nil value"
  end

  ## Accessors






































  def key(key, default \\ nil) do
    fn
      :get, data, next ->
        next.(Map.get(data, key, default))

      :get_and_update, data, next ->
        value = Map.get(data, key, default)

        case next.(value) do
          {get, update} -> {get, Map.put(data, key, update)}
          :pop -> {value, Map.delete(data, key)}
        end
    end
  end













































  def key!(key) do
    fn
      :get, %{} = data, next ->
        next.(Map.fetch!(data, key))

      :get_and_update, %{} = data, next ->
        value = Map.fetch!(data, key)

        case next.(value) do
          {get, update} -> {get, Map.put(data, key, update)}
          :pop -> {value, Map.delete(data, key)}
        end

      _op, data, _next ->
        raise "Access.key!/1 expected a map/struct, got: #{inspect(data)}"
    end
  end































  def elem(index) when is_integer(index) and index >= 0 do
    pos = index + 1

    fn
      :get, data, next when is_tuple(data) ->
        next.(:erlang.element(pos, data))

      :get_and_update, data, next when is_tuple(data) ->
        value = :erlang.element(pos, data)

        case next.(value) do
          {get, update} -> {get, :erlang.setelement(pos, data, update)}
          :pop -> raise "cannot pop data from a tuple"
        end

      _op, data, _next ->
        raise "Access.elem/1 expected a tuple, got: #{inspect(data)}"
    end
  end



































  def all() do
    &all/3
  end

  defp all(:get, data, next) when is_list(data) do
    Enum.map(data, next)
  end

  defp all(:get_and_update, data, next) when is_list(data) do
    all(data, next, _gets = [], _updates = [])
  end

  defp all(_op, data, _next) do
    raise "Access.all/0 expected a list, got: #{inspect(data)}"
  end

  defp all([head | rest], next, gets, updates) do
    case next.(head) do
      {get, update} -> all(rest, next, [get | gets], [update | updates])
      :pop -> all(rest, next, [head | gets], updates)
    end
  end

  defp all([], _next, gets, updates) do
    {:lists.reverse(gets), :lists.reverse(updates)}
  end





















































  def at(index) when is_integer(index) do
    fn op, data, next -> at(op, data, index, next) end
  end

  defp at(:get, data, index, next) when is_list(data) do
    data |> Enum.at(index) |> next.()
  end

  defp at(:get_and_update, data, index, next) when is_list(data) do
    get_and_update_at(data, index, next, [], fn -> nil end)
  end

  defp at(_op, data, _index, _next) do
    raise "Access.at/1 expected a list, got: #{inspect(data)}"
  end

  defp get_and_update_at([head | rest], 0, next, updates, _default_fun) do
    case next.(head) do
      {get, update} -> {get, :lists.reverse([update | updates], rest)}
      :pop -> {head, :lists.reverse(updates, rest)}
    end
  end

  defp get_and_update_at([_ | _] = list, index, next, updates, default_fun) when index < 0 do
    list_length = length(list)

    if list_length + index >= 0 do
      get_and_update_at(list, list_length + index, next, updates, default_fun)
    else
      {default_fun.(), list}
    end
  end

  defp get_and_update_at([head | rest], index, next, updates, default_fun) when index > 0 do
    get_and_update_at(rest, index - 1, next, [head | updates], default_fun)
  end

  defp get_and_update_at([], _index, _next, updates, default_fun) do
    {default_fun.(), :lists.reverse(updates)}
  end















  def at!(index) when is_integer(index) do
    fn op, data, next -> at!(op, data, index, next) end
  end

  defp at!(:get, data, index, next) when is_list(data) do
    case Enum.fetch(data, index) do
      {:ok, value} -> next.(value)
      :error -> raise Enum.OutOfBoundsError
    end
  end

  defp at!(:get_and_update, data, index, next) when is_list(data) do
    get_and_update_at(data, index, next, [], fn -> raise Enum.OutOfBoundsError end)
  end

  defp at!(_op, data, _index, _next) do
    raise "Access.at!/1 expected a list, got: #{inspect(data)}"
  end

















































  def filter(func) when is_function(func) do
    fn op, data, next -> filter(op, data, func, next) end
  end

  defp filter(:get, data, func, next) when is_list(data) do
    data |> Enum.filter(func) |> Enum.map(next)
  end

  defp filter(:get_and_update, data, func, next) when is_list(data) do
    get_and_update_filter(data, func, next, [], [])
  end

  defp filter(_op, data, _func, _next) do
    raise "Access.filter/1 expected a list, got: #{inspect(data)}"
  end

  defp get_and_update_filter([head | rest], func, next, updates, gets) do
    if func.(head) do
      case next.(head) do
        {get, update} ->
          get_and_update_filter(rest, func, next, [update | updates], [get | gets])

        :pop ->
          get_and_update_filter(rest, func, next, updates, [head | gets])
      end
    else
      get_and_update_filter(rest, func, next, [head | updates], gets)
    end
  end

  defp get_and_update_filter([], _func, _next, updates, gets) do
    {:lists.reverse(gets), :lists.reverse(updates)}
  end



















































  def slice(%Range{} = range) do
    if range.step > 0 do
      fn op, data, next -> slice(op, data, range, next) end
    else
      raise ArgumentError,
            "Access.slice/1 does not accept ranges with negative steps, got: #{inspect(range)}"
    end
  end

  defp slice(:get, data, %Range{} = range, next) when is_list(data) do
    data
    |> Enum.slice(range)
    |> Enum.map(next)
  end

  defp slice(:get_and_update, data, range, next) when is_list(data) do
    range = normalize_range(range, data)

    if range.first > range.last do
      {[], data}
    else
      get_and_update_slice(data, range, next, [], [], 0)
    end
  end

  defp slice(_op, data, _range, _next) do
    raise ArgumentError, "Access.slice/1 expected a list, got: #{inspect(data)}"
  end

  defp normalize_range(%Range{first: first, last: last, step: step}, list)
       when first < 0 or last < 0 do
    count = length(list)
    first = if first >= 0, do: first, else: Kernel.max(first + count, 0)
    last = if last >= 0, do: last, else: last + count
    Range.new(first, last, step)
  end

  defp normalize_range(range, _list), do: range

  defp get_and_update_slice([head | rest], range, next, updates, gets, index) do
    if index in range do
      case next.(head) do
        :pop ->
          get_and_update_slice(rest, range, next, updates, [head | gets], index + 1)

        {get, update} ->
          get_and_update_slice(
            rest,
            range,
            next,
            [update | updates],
            [get | gets],
            index + 1
          )
      end
    else
      get_and_update_slice(rest, range, next, [head | updates], gets, index + 1)
    end
  end

  defp get_and_update_slice([], _range, _next, updates, gets, _index) do
    {:lists.reverse(gets), :lists.reverse(updates)}
  end














































  def find(predicate) when is_function(predicate, 1) do
    fn op, data, next -> find(op, data, predicate, next) end
  end

  defp find(:get, data, predicate, next) when is_list(data) do
    data |> Enum.find(predicate) |> next.()
  end

  defp find(:get_and_update, data, predicate, next) when is_list(data) do
    get_and_update_find(data, [], predicate, next)
  end

  defp find(_op, data, _predicate, _next) do
    raise "Access.find/1 expected a list, got: #{inspect(data)}"
  end

  defp get_and_update_find([], updates, _predicate, _next) do
    {nil, :lists.reverse(updates)}
  end

  defp get_and_update_find([head | rest], updates, predicate, next) do
    if predicate.(head) do
      case next.(head) do
        {get, update} -> {get, :lists.reverse([update | updates], rest)}
        :pop -> {head, :lists.reverse(updates, rest)}
      end
    else
      get_and_update_find(rest, [head | updates], predicate, next)
    end
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/access.ex (docs and specs stripped;
# line numbers match the original).
