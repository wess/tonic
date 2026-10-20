defmodule Tonic.Der do
  @moduledoc false
  # DER primitives: tag-length-value items, integers, bit strings and object
  # identifiers.

  def body(der) do
    {_tag, value, _raw, _rest} = tlv(der)
    value
  end

  def items(""), do: []

  def items(der) do
    {tag, value, raw, rest} = tlv(der)
    [{tag, value, raw} | items(rest)]
  end

  defp tlv(<<tag, rest::binary>> = der) do
    {length, rest} =
      case rest do
        <<0::1, n::7, rest::binary>> ->
          {n, rest}

        <<1::1, k::7, rest::binary>> ->
          <<n::size(k * 8), rest::binary>> = rest
          {n, rest}
      end

    <<value::binary-size(length), rest::binary>> = rest
    {tag, value, binary_part(der, 0, byte_size(der) - byte_size(rest)), rest}
  end

  def integer(bytes) do
    size = bit_size(bytes)
    <<n::signed-size(size)>> = bytes
    n
  end

  def bit_string(<<_unused, bits::binary>>), do: bits

  def oid(<<first, rest::binary>>) do
    List.to_tuple([div(first, 40), rem(first, 40) | arcs(rest, 0)])
  end

  defp arcs("", _), do: []
  defp arcs(<<1::1, n::7, rest::binary>>, acc), do: arcs(rest, acc * 128 + n)
  defp arcs(<<0::1, n::7, rest::binary>>, acc), do: [acc * 128 + n | arcs(rest, 0)]
end
