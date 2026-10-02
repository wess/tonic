defmodule Integer do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.












  import Bitwise
























  defguard is_odd(integer) when is_integer(integer) and (integer &&& 1) == 1
























  defguard is_even(integer) when is_integer(integer) and (integer &&& 1) == 0







































  def pow(base, exponent) when is_integer(base) and is_integer(exponent) do
    if exponent < 0, do: :erlang.error(:badarith, [base, exponent])
    base ** exponent
  end




















  def mod(dividend, divisor) do
    remainder = rem(dividend, divisor)

    if remainder * divisor < 0 do
      remainder + divisor
    else
      remainder
    end
  end

























  def floor_div(dividend, divisor) do
    if :erlang.xor(dividend < 0, divisor < 0) and rem(dividend, divisor) != 0 do
      div(dividend, divisor) - 1
    else
      div(dividend, divisor)
    end
  end




















  def digits(integer, base \\ 10)
      when is_integer(integer) and is_integer(base) and base >= 2 do
    case integer do
      0 -> [0]
      _integer -> digits(integer, base, [])
    end
  end

  defp digits(0, _base, acc), do: acc

  defp digits(integer, base, acc),
    do: digits(div(integer, base), base, [rem(integer, base) | acc])




















  def undigits(digits, base \\ 10) when is_list(digits) and is_integer(base) and base >= 2 do
    undigits(digits, base, 0)
  end

  defp undigits([], _base, acc), do: acc

  defp undigits([digit | _], base, _) when is_integer(digit) and digit >= base,
    do: raise(ArgumentError, "invalid digit #{digit} in base #{base}")

  defp undigits([digit | tail], base, acc) when is_integer(digit),
    do: undigits(tail, base, acc * base + digit)











































  def parse(binary, base \\ 10)

  def parse(_binary, base) when base not in 2..36 do
    raise ArgumentError, "invalid base #{inspect(base)}"
  end

  def parse(binary, base) when is_binary(binary) do
    case count_digits(binary, base) do
      0 ->
        :error

      count ->
        {digits, rem} = :erlang.split_binary(binary, count)
        {:erlang.binary_to_integer(digits, base), rem}
    end
  end

  defp count_digits(<<sign, rest::bits>>, base) when sign in ~c"+-" do
    case count_digits_nosign(rest, base, 1) do
      1 -> 0
      count -> count
    end
  end

  defp count_digits(<<rest::bits>>, base) do
    count_digits_nosign(rest, base, 0)
  end

  digits = [{?0..?9, -?0}, {?A..?Z, 10 - ?A}, {?a..?z, 10 - ?a}]

  for {chars, diff} <- digits,
      char <- chars do
    digit = char + diff

    defp count_digits_nosign(<<unquote(char), rest::bits>>, base, count)
         when base > unquote(digit) do
      count_digits_nosign(rest, base, count + 1)
    end
  end

  defp count_digits_nosign(<<_::bits>>, _, count), do: count



































  def to_string(integer, base \\ 10) do
    :erlang.integer_to_binary(integer, base)
  end



































  def to_charlist(integer, base \\ 10) do
    :erlang.integer_to_list(integer, base)
  end
































  def gcd(integer1, integer2) when is_integer(integer1) and is_integer(integer2) do
    gcd_positive(abs(integer1), abs(integer2))
  end

  defp gcd_positive(0, integer2), do: integer2
  defp gcd_positive(integer1, 0), do: integer1
  defp gcd_positive(integer1, integer2), do: gcd_positive(integer2, rem(integer1, integer2))



































  def extended_gcd(0, 0), do: {0, 0, 0}
  def extended_gcd(0, b), do: {b, 0, 1}
  def extended_gcd(a, 0), do: {a, 1, 0}

  def extended_gcd(integer1, integer2) when is_integer(integer1) and is_integer(integer2) do
    extended_gcd(integer2, integer1, 0, 1, 1, 0)
  end

  defp extended_gcd(r1, r0, s1, s0, t1, t0) do
    div = div(r0, r1)

    case r0 - div * r1 do
      0 when r1 > 0 -> {r1, s1, t1}
      0 when r1 < 0 -> {-r1, -s1, -t1}
      r2 -> extended_gcd(r2, r1, s0 - div * s1, s1, t0 - div * t1, t1)
    end
  end



  def to_char_list(integer), do: Integer.to_charlist(integer)



  def to_char_list(integer, base), do: Integer.to_charlist(integer, base)
end

# Imported from Elixir 1.18.3 lib/elixir/lib/integer.ex (docs and specs stripped;
# line numbers match the original).
