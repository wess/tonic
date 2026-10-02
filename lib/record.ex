defmodule Record do
  defmacro defrecord(name, fields), do: Record.definitions(:defmacro, name, name, fields)
  defmacro defrecord(name, tag, fields), do: Record.definitions(:defmacro, name, tag, fields)
  defmacro defrecordp(name, fields), do: Record.definitions(:defmacrop, name, name, fields)
  defmacro defrecordp(name, tag, fields), do: Record.definitions(:defmacrop, name, tag, fields)

  def definitions(kind, name, tag, fields) do
    fields = Enum.map(fields, fn
      field when is_atom(field) -> {field, nil}
      {field, default} when is_atom(field) -> {field, default}
      _ -> raise ArgumentError, "invalid record field"
    end)
    unless is_atom(name) and is_atom(tag) and Keyword.keyword?(fields) do
      raise ArgumentError, "record name and tag must be atoms and fields must be a keyword list"
    end
    Enum.map([0, 1, 2], fn arity ->
      arguments = if arity == 0, do: [], else: Enum.map(1..arity, fn index -> {String.to_atom("record_argument#{index}"), [], nil} end)
      caller_context = {{:., [], [Map, :get]}, [], [{:__CALLER__, [], nil}, :context]}
      call = {{:., [], [Record, :expand]}, [], [tag, Macro.escape(fields), arguments, caller_context]}
      {kind, [], [{name, [], arguments}, [do: call]]}
    end)
  end

  def expand(tag, fields, [], context), do: expand(tag, fields, [[]], context)
  def expand(_tag, fields, [field], _context) when is_atom(field) do
    index(fields, field)
  end
  def expand(tag, fields, [values], context) when is_list(values) do
    validate(fields, values)
    entries = Enum.map(fields, fn {name, default} ->
      Keyword.get(values, name, if(context == :match, do: {:_, [], nil}, else: Macro.escape(default)))
    end)
    {:{}, [], [tag | entries]}
  end
  def expand(_tag, fields, [record, field], _context) when is_atom(field) do
    {{:., [], [:erlang, :element]}, [], [index(fields, field) + 1, record]}
  end
  def expand(_tag, fields, [record, values], _context) when is_list(values) do
    validate(fields, values)
    Enum.reduce(values, record, fn {field, value}, result ->
      {{:., [], [:erlang, :setelement]}, [], [index(fields, field) + 1, result, value]}
    end)
  end
  def expand(_tag, _fields, _args, _context), do: raise(ArgumentError, "invalid record arguments")

  defp index(fields, name) do
    case Enum.find_index(fields, fn {field, _} -> field == name end) do
      nil -> raise ArgumentError, "unknown record field #{inspect(name)}"
      index -> index + 1
    end
  end
  defp validate(fields, values) do
    unless Keyword.keyword?(values), do: raise(ArgumentError, "record fields must be a keyword list")
    Enum.each(values, fn {name, _} -> index(fields, name) end)
  end
end
