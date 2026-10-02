defprotocol JSON.Encoder do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.



























  @undefined_impl_description """
  the protocol must be explicitly implemented.

  If you have a struct, you can derive the implementation specifying \
  which fields should be encoded to JSON:

      @derive {JSON.Encoder, only: [....]}
      defstruct ...

  It is also possible to encode all fields, although this should be \
  used carefully to avoid accidentally leaking private information \
  when new fields are added:

      @derive JSON.Encoder
      defstruct ...

  Finally, if you don't own the struct you want to encode to JSON, \
  you may use Protocol.derive/3 placed outside of any module:

      Protocol.derive(JSON.Encoder, NameOfTheStruct, only: [...])
      Protocol.derive(JSON.Encoder, NameOfTheStruct)\
  """


  defimpl JSON.Encoder, for: Any do
    def encode(%mod{} = struct, encoder) do
      fields = mod.__info__(:struct) |> Enum.map(& &1.field)
      fields = JSON.Encoder.__fields_to_encode__(fields, mod.__derive_opts__(JSON.Encoder))

      {io, _prefix} =
        Enum.flat_map_reduce(fields, ?{, fn field, prefix ->
          key = IO.iodata_to_binary([prefix, :elixir_json.encode_binary(Atom.to_string(field)), ?:])
          {[key, encoder.(Map.fetch!(struct, field), encoder)], ?,}
        end)

      if io == [], do: "{}", else: io ++ [?}]
    end
  end









  def __fields_to_encode__(fields, opts) do
    cond do
      only = Keyword.get(opts, :only) ->
        case only -- fields do
          [] ->
            only

          error_keys ->
            raise ArgumentError,
                  "unknown struct fields #{inspect(error_keys)} specified in :only. Expected one of: " <>
                    "#{inspect(fields -- [:__struct__])}"
        end

      except = Keyword.get(opts, :except) ->
        case except -- fields do
          [] ->
            fields -- [:__struct__ | except]

          error_keys ->
            raise ArgumentError,
                  "unknown struct fields #{inspect(error_keys)} specified in :except. Expected one of: " <>
                    "#{inspect(fields -- [:__struct__])}"
        end

      true ->
        fields -- [:__struct__]
    end
  end




  def encode(term, encoder)
end

defimpl JSON.Encoder, for: Atom do
  def encode(value, encoder) do
    case value do
      nil -> "null"
      true -> "true"
      false -> "false"
      _ -> encoder.(Atom.to_string(value), encoder)
    end
  end
end

defimpl JSON.Encoder, for: BitString do
  def encode(value, _encoder) do
    :elixir_json.encode_binary(value)
  end
end

defimpl JSON.Encoder, for: List do
  def encode(value, encoder) do
    :elixir_json.encode_list(value, encoder)
  end
end

defimpl JSON.Encoder, for: Integer do
  def encode(value, _encoder) do
    :elixir_json.encode_integer(value)
  end
end

defimpl JSON.Encoder, for: Float do
  def encode(value, _encoder) do
    :elixir_json.encode_float(value)
  end
end

defimpl JSON.Encoder, for: Map do
  def encode(value, encoder) do
    case :maps.next(:maps.iterator(value)) do
      :none ->
        "{}"

      {key, value, iterator} ->
        [?{, key(key, encoder), ?:, encoder.(value, encoder) | next(iterator, encoder)]
    end
  end

  defp next(iterator, encoder) do
    case :maps.next(iterator) do
      :none ->
        "}"

      {key, value, iterator} ->
        [?,, key(key, encoder), ?:, encoder.(value, encoder) | next(iterator, encoder)]
    end
  end

  # Erlang supports only numbers, binaries, and atoms as keys,
  # we support anything that implements the String.Chars protocol.
  defp key(key, encoder) when is_atom(key), do: encoder.(Atom.to_string(key), encoder)
  defp key(key, encoder) when is_binary(key), do: encoder.(key, encoder)
  defp key(key, encoder), do: encoder.(String.Chars.to_string(key), encoder)
end

defimpl JSON.Encoder, for: [Date, Time, NaiveDateTime, DateTime, Duration] do
  def encode(value, _encoder) do
    [?", @for.to_iso8601(value), ?"]
  end
end

defmodule JSON.DecodeError do



  defexception [:message, :offset, :data]
end

defmodule JSON do

































































  def decode(binary) when is_binary(binary) do
    with {decoded, :ok, rest} <- decode(binary, :ok, []) do
      if rest == "" do
        {:ok, decoded}
      else
        {:error, {:invalid_byte, byte_size(binary) - byte_size(rest), :binary.at(rest, 0)}}
      end
    end
  end



























  def decode(binary, acc, decoders) when is_binary(binary) and is_list(decoders) do
    decoders = Keyword.put_new(decoders, :null, nil)

    try do
      :elixir_json.decode(binary, acc, Map.new(decoders))
    catch
      :error, :unexpected_end ->
        {:error, {:unexpected_end, byte_size(binary)}}

      :error, {:invalid_byte, byte} ->
        {:error, {:invalid_byte, offset(__STACKTRACE__), byte}}

      :error, {:unexpected_sequence, bytes} ->
        {:error, {:unexpected_sequence, offset(__STACKTRACE__), bytes}}
    end
  end

  defp offset(stacktrace) do
    with [{_, _, _, opts} | _] <- stacktrace,
         %{cause: %{position: position}} <- opts[:error_info] do
      position
    else
      _ -> 0
    end
  end












  def decode!(binary) when is_binary(binary) do
    case decode(binary) do
      {:ok, decoded} ->
        decoded

      {:error, {:unexpected_end, offset}} ->
        raise JSON.DecodeError,
          message: "unexpected end of JSON binary at position (byte offset) #{offset}",
          data: binary,
          offset: offset

      {:error, {:invalid_byte, offset, byte}} ->
        raise JSON.DecodeError,
          message: "invalid byte #{byte} at position (byte offset) #{offset}",
          data: binary,
          offset: offset

      {:error, {:unexpected_sequence, offset, bytes}} ->
        raise JSON.DecodeError,
          message: "unexpected sequence #{inspect(bytes)} at position (byte offset) #{offset}",
          data: binary,
          offset: offset
    end
  end




















  def encode!(term, encoder \\ &protocol_encode/2) do
    IO.iodata_to_binary(encoder.(term, encoder))
  end


















  def encode_to_iodata!(term, encoder \\ &protocol_encode/2) do
    encoder.(term, encoder)
  end









  def protocol_encode(value, encoder) when is_atom(value) do
    case value do
      nil -> "null"
      true -> "true"
      false -> "false"
      _ -> encoder.(Atom.to_string(value), encoder)
    end
  end

  def protocol_encode(value, _encoder) when is_binary(value),
    do: :elixir_json.encode_binary(value)

  def protocol_encode(value, _encoder) when is_integer(value),
    do: :elixir_json.encode_integer(value)

  def protocol_encode(value, _encoder) when is_float(value),
    do: :elixir_json.encode_float(value)

  def protocol_encode(value, encoder) when is_list(value),
    do: :elixir_json.encode_list(value, encoder)

  def protocol_encode(%{} = value, encoder) when not is_map_key(value, :__struct__),
    do: JSON.Encoder.Map.encode(value, encoder)

  def protocol_encode(value, encoder),
    do: JSON.Encoder.encode(value, encoder)
end

# Imported from Elixir 1.18.3 lib/elixir/lib/json.ex (docs and specs stripped;
# line numbers match the original).
