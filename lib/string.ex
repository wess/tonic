defmodule Float do
  import Bitwise

  @power_of_2_to_52 4_503_599_627_370_496
  @precision_range 0..15

  def parse(binary), do: :tonic.float_parse(binary)
  def to_string(float), do: :tonic.float_short(float)
  def to_charlist(float), do: String.to_charlist(:tonic.float_short(float))

  def floor(number, precision \\ 0)

  def floor(number, 0) when is_float(number) do
    :math.floor(number)
  end

  def floor(number, precision) when is_float(number) and precision in @precision_range do
    round(number, precision, :floor)
  end

  def floor(number, precision) when is_float(number) do
    raise ArgumentError, invalid_precision_message(precision)
  end

  def ceil(number, precision \\ 0)

  def ceil(number, 0) when is_float(number) do
    :math.ceil(number)
  end

  def ceil(number, precision) when is_float(number) and precision in @precision_range do
    round(number, precision, :ceil)
  end

  def ceil(number, precision) when is_float(number) do
    raise ArgumentError, invalid_precision_message(precision)
  end

  def round(float, precision \\ 0)

  def round(float, 0) when float == 0.0, do: float

  def round(float, 0) when is_float(float) do
    case float |> :erlang.round() |> :erlang.float() do
      zero when zero == 0.0 and float < 0.0 -> -0.0
      rounded -> rounded
    end
  end

  def round(float, precision) when is_float(float) and precision in @precision_range do
    round(float, precision, :half_up)
  end

  def round(float, precision) when is_float(float) do
    raise ArgumentError, invalid_precision_message(precision)
  end

  defp round(num, _precision, _rounding) when is_float(num) and num == 0.0, do: num

  defp round(float, precision, rounding) do
    <<sign::1, exp::11, significant::52-bitstring>> = <<float::float>>
    {num, count} = decompose(significant, 1)
    count = count - exp + 1023

    cond do
      count >= 104 ->
        case rounding do
          :ceil when sign === 0 -> 1 / power_of_10(precision)
          :floor when sign === 1 -> -1 / power_of_10(precision)
          :ceil when sign === 1 -> minus_zero()
          :half_up when sign === 1 -> minus_zero()
          _ -> 0.0
        end

      count <= precision ->
        float

      true ->
        diff = count - precision - 1

        power_of_10 = power_of_10(diff)

        num = num * power_of_5(count)

        num = div(num, power_of_10)
        div = div(num, 10)
        num = rounding(rounding, sign, num, div)

        den = power_of_10(precision)
        boundary = den <<< 52

        cond do
          num == 0 and sign == 1 ->
            minus_zero()

          num == 0 ->
            0.0

          num >= boundary ->
            {den, exp} = scale_down(num, boundary, 52)
            decimal_to_float(sign, num, den, exp)

          true ->
            {num, exp} = scale_up(num, boundary, 52)
            decimal_to_float(sign, num, den, exp)
        end
    end
  end

  defp minus_zero, do: -0.0

  defp decompose(significant, initial) do
    decompose(significant, 1, 0, initial)
  end

  defp decompose(<<1::1, bits::bitstring>>, count, last_count, acc) do
    decompose(bits, count + 1, count, (acc <<< (count - last_count)) + 1)
  end

  defp decompose(<<0::1, bits::bitstring>>, count, last_count, acc) do
    decompose(bits, count + 1, last_count, acc)
  end

  defp decompose(<<>>, _count, last_count, acc) do
    {acc, last_count}
  end

  defp scale_up(num, boundary, exp) when num >= boundary, do: {num, exp}
  defp scale_up(num, boundary, exp), do: scale_up(num <<< 1, boundary, exp - 1)

  defp scale_down(num, den, exp) do
    new_den = den <<< 1

    if num < new_den do
      {den >>> 52, exp}
    else
      scale_down(num, new_den, exp + 1)
    end
  end

  defp decimal_to_float(sign, num, den, exp) do
    quo = div(num, den)
    rem = num - quo * den

    tmp =
      case den >>> 1 do
        den when rem > den -> quo + 1
        den when rem < den -> quo
        _ when (quo &&& 1) === 1 -> quo + 1
        _ -> quo
      end

    tmp = tmp - @power_of_2_to_52
    <<tmp::float>> = <<sign::1, exp + 1023::11, tmp::52>>
    tmp
  end

  defp rounding(:floor, 1, _num, div), do: div + 1
  defp rounding(:ceil, 0, _num, div), do: div + 1

  defp rounding(:half_up, _sign, num, div) do
    case rem(num, 10) do
      rem when rem < 5 -> div
      rem when rem >= 5 -> div + 1
    end
  end

  defp rounding(_, _, _, div), do: div

  defp power_of_10(x), do: Integer.pow(10, x)
  defp power_of_5(x), do: Integer.pow(5, x)

  defp invalid_precision_message(precision) do
    "precision #{precision} is out of valid range of #{inspect(@precision_range)}"
  end

  def pow(base, exponent), do: :math.pow(base, exponent)
  def min_finite, do: -1.7976931348623157e308
  def max_finite, do: 1.7976931348623157e308
  def ratio(float) when is_float(float) and float == 0.0, do: {0, 1}

  def ratio(float) when is_float(float) do
    <<sign::1, exp::11, mantissa::52>> = <<float::float>>

    {num, den_exp} =
      if exp != 0 do
        # Floats are expressed like this:
        # (2**52 + mantissa) * 2**(-52 + exp - 1023)
        #
        # We compute the root factors of the mantissa so we have this:
        # (2**52 + mantissa * 2**count) * 2**(-52 + exp - 1023)
        {mantissa, count} = root_factors(mantissa, 0)

        # Now we can move the count around so we have this:
        # (2**(52-count) + mantissa) * 2**(count + -52 + exp - 1023)
        if mantissa == 0 do
          {1, exp - 1023}
        else
          num = Bitwise.bsl(1, 52 - count) + mantissa
          den_exp = count - 52 + exp - 1023
          {num, den_exp}
        end
      else
        # Subnormals are expressed like this:
        # (mantissa) * 2**(-52 + 1 - 1023)
        #
        # So we compute it to this:
        # (mantissa * 2**(count)) * 2**(-52 + 1 - 1023)
        #
        # Which becomes:
        # mantissa * 2**(count-1074)
        root_factors(mantissa, -1074)
      end

    if den_exp > 0 do
      {sign(sign, Bitwise.bsl(num, den_exp)), 1}
    else
      {sign(sign, num), Bitwise.bsl(1, -den_exp)}
    end
  end

  defp root_factors(mantissa, count) when mantissa != 0 and Bitwise.band(mantissa, 1) == 0,
    do: root_factors(Bitwise.bsr(mantissa, 1), count + 1)

  defp root_factors(mantissa, count),
    do: {mantissa, count}

  @compile {:inline, sign: 2}
  defp sign(0, num), do: num
  defp sign(1, num), do: -num

  defp root_factors(mantissa, count) when mantissa != 0 and Bitwise.band(mantissa, 1) == 0,
    do: root_factors(Bitwise.bsr(mantissa, 1), count + 1)

  defp root_factors(mantissa, count),
    do: {mantissa, count}

  defp sign(0, num), do: num
  defp sign(1, num), do: -num


end

defmodule Bitwise do
  def band(a, b), do: :erlang.band(a, b)
  def bor(a, b), do: :erlang.bor(a, b)
  def bxor(a, b), do: :erlang.bxor(a, b)
  def bnot(a), do: :erlang.bnot(a)
  def bsl(a, b), do: :erlang.bsl(a, b)
  def bsr(a, b), do: :erlang.bsr(a, b)
  def left &&& right, do: :erlang.band(left, right)
  def left ||| right, do: :erlang.bor(left, right)
  def left ^^^ right, do: :erlang.bxor(left, right)
  def left <<< right, do: :erlang.bsl(left, right)
  def left >>> right, do: :erlang.bsr(left, right)
  def ~~~expr, do: :erlang.bnot(expr)
end

