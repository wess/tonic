defmodule IO do
  def puts(device \\ :stdio, item)

  def puts(:stdio, item) when is_binary(item) do
    case :tonic.group_leader_of(self()) do
      nil -> :tonic.io_write(:stdio, [item, ?\n])
      _ -> put_chars(:standard_io, [item, ?\n])
    end
  end
  def puts(device, item), do: put_chars(map_dev(device), [to_chardata(item), ?\n])

  def write(device \\ :stdio, chardata)
  def write(device, item), do: put_chars(map_dev(device), to_chardata(item))

  def binwrite(device \\ :stdio, iodata)

  def binwrite(device, iodata) when is_list(iodata) or is_binary(iodata) do
    with {:error, reason} <- :file.write(map_dev(device), iodata) do
      :erlang.error(reason)
    end
  end


  defp put_chars(device, chars) do
    case :file.io_put_chars(device, :unicode, chars) do
      {:error, reason} -> :erlang.error(conv_reason(reason))
      other -> other
    end
  end

  defp conv_reason(:arguments), do: :badarg
  defp conv_reason(:terminated), do: :terminated
  defp conv_reason(:calling_self), do: :calling_self
  defp conv_reason({:no_translation, _, _}), do: :no_translation
  defp conv_reason(:no_translation), do: :no_translation
  defp conv_reason(_), do: :badarg

  def inspect(item, opts \\ [])

  def inspect(item, opts) when is_list(opts), do: inspect(:stdio, item, opts)

  def inspect(device, item), do: inspect(device, item, [])

  def inspect(device, item, opts) when is_list(opts) do
    label = if label = opts[:label], do: [to_chardata(label), ": "], else: []
    opts = Inspect.Opts.new(Keyword.put_new(opts, :pretty, true))
    doc = Inspect.Algebra.group(Inspect.Algebra.to_doc(item, opts))
    chardata = Inspect.Algebra.format(doc, opts.width)
    puts(device, [label, chardata])
    item
  end

  def warn(message, %Macro.Env{line: line, file: file} = env) do
    message = to_chardata(message)

    :elixir_errors.emit_diagnostic(:warning, line, file, message, Macro.Env.stacktrace(env),
      read_snippet: true
    )
  end

  def warn(message, [{_, _} | _] = keyword) do
    if file = keyword[:file] do
      line = keyword[:line]
      column = keyword[:column]
      position = if line && column, do: {line, column}, else: line
      message = to_chardata(message)

      stacktrace =
        Macro.Env.stacktrace(%{
          __ENV__
          | module: keyword[:module],
            function: keyword[:function],
            line: line,
            file: file
        })

      :elixir_errors.emit_diagnostic(:warning, position, file, message, stacktrace,
        read_snippet: true
      )
    else
      warn(message, [])
    end
  end

  def warn(message, []) do
    message = to_chardata(message)
    :elixir_errors.emit_diagnostic(:warning, 0, nil, message, [], read_snippet: false)
  end

  def warn(message, [{_, _, _, _} | _] = stacktrace) do
    message = to_chardata(message)
    :elixir_errors.emit_diagnostic(:warning, 0, nil, message, stacktrace, read_snippet: false)
  end

  def warn(message) do
    {:current_stacktrace, stacktrace} = Process.info(self(), :current_stacktrace)
    warn(message, Enum.drop(stacktrace, 2))
  end

  def warn_once(key, message, _stacktrace_drop_levels) do
    if :ets.whereis(:tonic_warn_once) == :undefined do
      try do
        :ets.new(:tonic_warn_once, [:set, :public, :named_table])
      rescue
        _ -> :ok
      end
    end

    if :ets.insert_new(:tonic_warn_once, {key}) do
      warn(if(is_function(message), do: message.(), else: message))
    end

    :ok
  end

  def gets(device \\ :stdio, prompt)
  def gets(device, prompt), do: :file.io_get_line(map_dev(device), :unicode, to_chardata(prompt))

  def read(device \\ :stdio, line_or_chars)

  def read(device, :all) do
    IO.warn("IO.read(device, :all) is deprecated, use IO.read(device, :eof) instead")

    with :eof <- read(device, :eof) do
      case getopts(map_dev(device)) do
        [_ | _] = opts -> if Keyword.get(opts, :binary, true), do: "", else: ~c""
        _ -> ""
      end
    end
  end

  def read(device, :eof), do: getn(device, ~c"", :eof)
  def read(device, :line), do: :file.io_get_line(map_dev(device), :unicode)

  def read(device, count) when is_integer(count) and count >= 0,
    do: :file.io_get_chars(map_dev(device), :unicode, count)

  defp getopts(pid) when is_pid(pid), do: Tonic.FileIO.request(pid, :getopts)
  defp getopts(_), do: [binary: true, encoding: :unicode]

  def binread(device \\ :stdio, line_or_chars)

  def binread(device, :all) do
    IO.warn("IO.binread(device, :all) is deprecated, use IO.binread(device, :eof) instead")
    with :eof <- binread(device, :eof), do: ""
  end

  def binread(device, :eof), do: binread_eof(map_dev(device), "")

  def binread(device, :line) do
    case :file.read_line(map_dev(device)) do
      {:ok, data} -> data
      other -> other
    end
  end

  def binread(device, count) when is_integer(count) and count >= 0 do
    case :file.read(map_dev(device), count) do
      {:ok, data} -> data
      other -> other
    end
  end

  defp binread_eof(mapped_dev, acc) do
    case :file.read(mapped_dev, 4096) do
      {:ok, data} -> binread_eof(mapped_dev, acc <> IO.iodata_to_binary(data))
      :eof -> if acc == "", do: :eof, else: acc
      other -> other
    end
  end

  def getn(prompt, count \\ 1)
  def getn(prompt, :eof), do: getn(:stdio, prompt, :eof)
  def getn(prompt, count) when is_integer(count) and count > 0, do: getn(:stdio, prompt, count)
  def getn(device, prompt) when not is_integer(prompt), do: getn(device, prompt, 1)

  def getn(device, prompt, :eof), do: getn_eof(map_dev(device), to_chardata(prompt), [])

  def getn(device, prompt, count) when is_integer(count) and count > 0,
    do: :file.io_get_chars(map_dev(device), :unicode, to_chardata(prompt), count)

  defp getn_eof(device, prompt, acc) do
    case :file.io_get_line(device, :unicode, prompt) do
      line when is_binary(line) or is_list(line) -> getn_eof(device, ~c"", [line | acc])
      :eof -> wrap_eof(:lists.reverse(acc))
      other -> other
    end
  end

  defp wrap_eof([h | _] = acc) when is_binary(h), do: IO.iodata_to_binary(acc)
  defp wrap_eof([h | _] = acc) when is_list(h), do: List.flatten(acc)
  defp wrap_eof([]), do: :eof

  def stream, do: stream(:stdio, :line)

  def stream(device \\ :stdio, line_or_codepoints)
      when line_or_codepoints == :line or (is_integer(line_or_codepoints) and line_or_codepoints > 0) do
    IO.Stream.__build__(map_dev(device), false, line_or_codepoints)
  end

  def binstream, do: binstream(:stdio, :line)

  def binstream(device \\ :stdio, line_or_bytes)
      when line_or_bytes == :line or (is_integer(line_or_bytes) and line_or_bytes > 0) do
    IO.Stream.__build__(map_dev(device), true, line_or_bytes)
  end

  def each_stream(device, line_or_codepoints) do
    case read(device, line_or_codepoints) do
      :eof -> {:halt, device}
      {:error, reason} -> raise IO.StreamError, reason: reason
      data -> {[data], device}
    end
  end

  def each_binstream(device, line_or_chars) do
    case binread(device, line_or_chars) do
      :eof -> {:halt, device}
      {:error, reason} -> raise IO.StreamError, reason: reason
      data -> {[data], device}
    end
  end

  def iodata_to_binary(data), do: :erlang.iolist_to_binary(data)
  def iodata_length(data), do: :erlang.iolist_size(data)
  def chardata_to_string(data) when is_binary(data), do: data
  def chardata_to_string(data), do: :unicode.characters_to_binary(data)

  defp to_chardata(item) when is_binary(item), do: item
  defp to_chardata(item) when is_list(item), do: item
  defp to_chardata(item), do: String.Chars.to_string(item)

  defp map_dev(:stdio), do: :standard_io
  defp map_dev(:stderr), do: :standard_error
  defp map_dev(other), do: other
end

defmodule IO.StreamError do
  defexception [:reason]

  def message(%{reason: reason}), do: "error during streaming: #{inspect(reason)}"
end

defmodule IO.Stream do
  defstruct device: nil, raw: true, line_or_bytes: :line

  def __build__(device, raw, line_or_bytes) do
    _ = IO.Stream
    %IO.Stream{device: device, raw: raw, line_or_bytes: line_or_bytes}
  end

  defimpl Collectable do
    def into(%{device: device, raw: raw} = stream) do
      {:ok, into(stream, device, raw)}
    end

    defp into(stream, device, raw) do
      fn
        :ok, {:cont, x} ->
          case raw do
            true -> IO.binwrite(device, x)
            false -> IO.write(device, x)
          end

        :ok, _ ->
          stream
      end
    end
  end

  defimpl Enumerable do
    def reduce(%{device: device, raw: raw, line_or_bytes: line_or_bytes}, acc, fun) do
      next_fun =
        case raw do
          true -> &IO.each_binstream(&1, line_or_bytes)
          false -> &IO.each_stream(&1, line_or_bytes)
        end

      Stream.resource(fn -> device end, next_fun, & &1).(acc, fun)
    end

    def count(_stream), do: {:error, __MODULE__}
    def member?(_stream, _term), do: {:error, __MODULE__}
    def slice(_stream), do: {:error, __MODULE__}
  end
end


defmodule Tonic.StdIO do
  @moduledoc false
  # The default group leader: an I/O server on the standard streams.

  def pid do
    case Process.whereis(:"$tonic_stdio") do
      nil ->
        pid = spawn(fn -> loop() end)

        try do
          Process.register(pid, :"$tonic_stdio")
          pid
        rescue
          _ ->
            Process.exit(pid, :kill)
            Process.whereis(:"$tonic_stdio")
        end

      pid ->
        pid
    end
  end

  defp loop do
    receive do
      {:io_request, from, ref, req} ->
        send(from, {:io_reply, ref, handle(req)})
        loop()

      _ ->
        loop()
    end
  end

  defp handle({:put_chars, _enc, chars}), do: :tonic.io_write(:stdio, chars)
  defp handle({:put_chars, _enc, m, f, a}), do: :tonic.io_write(:stdio, apply(m, f, a))
  defp handle({:put_chars, chars}), do: :tonic.io_write(:stdio, chars)
  defp handle({:get_line, enc, prompt}), do: :file.io_get_line(:user, enc, prompt)
  defp handle({:get_chars, enc, prompt, n}), do: :file.io_get_chars(:user, enc, prompt, n)
  defp handle(:getopts), do: [binary: true, encoding: :unicode]
  defp handle({:setopts, _}), do: :ok
  defp handle({:requests, reqs}), do: Enum.reduce(reqs, :ok, fn r, _ -> handle(r) end)
  defp handle(_), do: {:error, :request}
end
