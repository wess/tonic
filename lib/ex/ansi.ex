defmodule IO.ANSI.Sequence do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.

  defmacro defsequence(name, code, terminator \\ "m") do
    quote bind_quoted: [name: name, code: code, terminator: terminator] do

      def unquote(name)() do
        "\e[#{unquote(code)}#{unquote(terminator)}"
      end

      defp format_sequence(unquote(name)) do
        unquote(name)()
      end
    end
  end
end

defmodule IO.ANSI do

































  import IO.ANSI.Sequence















  def enabled? do
    Application.get_env(:elixir, :ansi_enabled, false)
  end

























  def syntax_colors do
    Application.fetch_env!(:elixir, :ansi_syntax_colors)
  end



  def color(code) when code in 0..255, do: "\e[38;5;#{code}m"







  def color(r, g, b) when r in 0..5 and g in 0..5 and b in 0..5 do
    color(16 + 36 * r + 6 * g + b)
  end



  def color_background(code) when code in 0..255, do: "\e[48;5;#{code}m"







  def color_background(r, g, b) when r in 0..5 and g in 0..5 and b in 0..5 do
    color_background(16 + 36 * r + 6 * g + b)
  end


  defsequence(:reset, 0)


  defsequence(:bright, 1)


  defsequence(:faint, 2)


  defsequence(:italic, 3)


  defsequence(:underline, 4)


  defsequence(:blink_slow, 5)


  defsequence(:blink_rapid, 6)


  defsequence(:inverse, 7)


  defsequence(:reverse, 7)


  defsequence(:conceal, 8)


  defsequence(:crossed_out, 9)


  defsequence(:primary_font, 10)

  for font_n <- [1, 2, 3, 4, 5, 6, 7, 8, 9] do

    defsequence(:"font_#{font_n}", font_n + 10)
  end


  defsequence(:normal, 22)


  defsequence(:not_italic, 23)


  defsequence(:no_underline, 24)


  defsequence(:blink_off, 25)


  defsequence(:inverse_off, 27)


  defsequence(:reverse_off, 27)

  colors = [:black, :red, :green, :yellow, :blue, :magenta, :cyan, :white]

  for {color, code} <- Enum.with_index(colors) do

    defsequence(color, code + 30)


    defsequence(:"light_#{color}", code + 90)


    defsequence(:"#{color}_background", code + 40)


    defsequence(:"light_#{color}_background", code + 100)
  end


  defsequence(:default_color, 39)


  defsequence(:default_background, 49)


  defsequence(:framed, 51)


  defsequence(:encircled, 52)


  defsequence(:overlined, 53)


  defsequence(:not_framed_encircled, 54)


  defsequence(:not_overlined, 55)


  defsequence(:home, "", "H")







  def cursor(line, column)
      when is_integer(line) and line >= 0 and is_integer(column) and column >= 0 do
    "\e[#{line};#{column}H"
  end



  def cursor_up(lines \\ 1) when is_integer(lines) and lines >= 1, do: "\e[#{lines}A"



  def cursor_down(lines \\ 1) when is_integer(lines) and lines >= 1, do: "\e[#{lines}B"



  def cursor_right(columns \\ 1) when is_integer(columns) and columns >= 1, do: "\e[#{columns}C"



  def cursor_left(columns \\ 1) when is_integer(columns) and columns >= 1, do: "\e[#{columns}D"


  defsequence(:clear, "2", "J")


  defsequence(:clear_line, "2", "K")

  defp format_sequence(other) do
    raise ArgumentError, "invalid ANSI sequence specification: #{inspect(other)}"
  end























  def format(ansidata, emit? \\ enabled?()) when is_boolean(emit?) do
    do_format(ansidata, [], [], emit?, :maybe)
  end


















  def format_fragment(ansidata, emit? \\ enabled?()) when is_boolean(emit?) do
    do_format(ansidata, [], [], emit?, false)
  end

  defp do_format([term | rest], rem, acc, emit?, append_reset) do
    do_format(term, [rest | rem], acc, emit?, append_reset)
  end

  defp do_format(term, rem, acc, true, append_reset) when is_atom(term) do
    do_format([], rem, [acc | format_sequence(term)], true, !!append_reset)
  end

  defp do_format(term, rem, acc, false, append_reset) when is_atom(term) do
    format_sequence(term)
    do_format([], rem, acc, false, append_reset)
  end

  defp do_format(term, rem, acc, emit?, append_reset) when not is_list(term) do
    do_format([], rem, [acc, term], emit?, append_reset)
  end

  defp do_format([], [next | rest], acc, emit?, append_reset) do
    do_format(next, rest, acc, emit?, append_reset)
  end

  defp do_format([], [], acc, true, true) do
    [acc | IO.ANSI.reset()]
  end

  defp do_format([], [], acc, _emit?, _append_reset) do
    acc
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/io/ansi.ex (docs and specs stripped;
# line numbers match the original).
