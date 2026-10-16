defmodule Record do
  defmacro defrecord(name, fields), do: Record.definitions(:defmacro, name, name, fields)
  defmacro defrecord(name, tag, fields), do: Record.definitions(:defmacro, name, tag, fields)
  defmacro defrecordp(name, fields), do: Record.definitions(:defmacrop, name, name, fields)
  defmacro defrecordp(name, tag, fields), do: Record.definitions(:defmacrop, name, tag, fields)

  # OTP header records, as Record.extract/2 reads them from the headers.
  @otp_records %{
    {"kernel/include/file.hrl", :file_info} =>
      [:size, :type, :access, :atime, :mtime, :ctime, :mode, :links, :major_device, :minor_device, :inode, :uid, :gid],
    {"kernel/include/file.hrl", :file_descriptor} => [:module, :data]
  }

  def extract(name, opts) when is_atom(name) and is_list(opts) do
    source = opts[:from_lib] || opts[:from]

    case Map.fetch(@otp_records, {source, name}) do
      {:ok, fields} -> Enum.map(fields, &{&1, :undefined})
      :error -> raise ArgumentError, "no record #{name} found at #{source}"
    end
  end

  def extract_all(opts) when is_list(opts) do
    source = opts[:from_lib] || opts[:from]
    for {{^source, name}, _} <- @otp_records, do: {name, extract(name, opts)}
  end

  def definitions(kind, name, tag, fields) do
    fields = if literal_fields?(fields), do: fields, else: elem(Code.eval_quoted(fields), 0)
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

  defp literal_fields?(fields) when is_list(fields) do
    Enum.all?(fields, fn
      field when is_atom(field) -> true
      {field, _} when is_atom(field) -> true
      _ -> false
    end)
  end

  defp literal_fields?(_), do: false

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
  # user(record): the record as a keyword list
  def expand(tag, fields, [record], _context) do
    {{:., [], [Record, :__keyword__]}, [], [tag, Keyword.keys(fields), record]}
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

  @doc false
  def __keyword__(tag, names, record) do
    if is_tuple(record) and tuple_size(record) == length(names) + 1 and elem(record, 0) == tag do
      Enum.zip(names, tl(Tuple.to_list(record)))
    else
      raise ArgumentError,
            "expected argument to be a literal atom, literal keyword or a #{inspect(tag)} record, got runtime: #{inspect(record)}"
    end
  end

  defguard is_record(data) when is_tuple(data) and tuple_size(data) > 0 and is_atom(elem(data, 0))

  defguard is_record(data, kind)
           when is_atom(kind) and is_tuple(data) and tuple_size(data) > 0 and elem(data, 0) == kind

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
