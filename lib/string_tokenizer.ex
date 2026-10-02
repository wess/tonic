# String.Tokenizer (lib/elixir/unicode/tokenizer.ex) with the Unicode
# tables replaced by a native identifier-class lookup. Script sets are not
# tracked (every non-ASCII identifier character counts as Common), so the
# mixed-script check never triggers.
defmodule String.Tokenizer do
  @moduledoc false
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
  @bottom 0
  @latin 1
  @top -1

  defp ss_latin(ss), do: :erlang.band(ss, @latin)
  defp ss_intersect(left, right), do: :erlang.band(left, right)

  defp ascii_upper?(entry), do: entry >= ?A and entry <= ?Z
  defp ascii_lower?(entry), do: entry >= ?a and entry <= ?z
  defp ascii_continue?(entry), do: entry >= ?0 and entry <= ?9

  defp unicode_upper(c), do: if(:tonic.ident_class(c) == 1, do: @top, else: @bottom)
  defp unicode_start(c), do: if(:tonic.ident_class(c) == 2, do: @top, else: @bottom)
  defp unicode_continue(c), do: if(:tonic.ident_class(c) == 3, do: @top, else: @bottom)

  def dir(i) when i in 48..57, do: :weak_number
  def dir(i) when is_integer(i), do: :ltr

  defp normalize_start(?µ), do: {?μ, @top}
  defp normalize_start(_codepoint), do: @bottom

  def tokenize([head | tail]) do
    cond do
      ascii_upper?(head) ->
        validate(continue(tail, [head], 1, true, @latin, []), :alias)

      ascii_lower?(head) ->
        validate(continue(tail, [head], 1, true, @latin, []), :identifier)

      head == ?_ ->
        validate(continue(tail, [head], 1, true, @top, []), :identifier)

      true ->
        case unicode_upper(head) do
          @bottom ->
            case unicode_start(head) do
              @bottom ->
                case normalize_start(head) do
                  @bottom ->
                    {:error, :empty}

                  {head, scriptset} ->
                    validate(continue(tail, [head], 1, false, scriptset, [:nfkc]), :identifier)
                end

              scriptset ->
                validate(continue(tail, [head], 1, false, scriptset, []), :identifier)
            end

          scriptset ->
            validate(continue(tail, [head], 1, false, scriptset, []), :atom)
        end
    end
  end

  def tokenize([]) do
    {:error, :empty}
  end

  defp continue([?! | tail], acc, length, ascii_letters?, scriptset, special) do
    {[?! | acc], tail, length + 1, ascii_letters?, scriptset, [:punctuation | special]}
  end

  defp continue([?? | tail], acc, length, ascii_letters?, scriptset, special) do
    {[?? | acc], tail, length + 1, ascii_letters?, scriptset, [:punctuation | special]}
  end

  defp continue([?@ | tail], acc, length, ascii_letters?, scriptset, special) do
    special = [:at | List.delete(special, :at)]
    continue(tail, [?@ | acc], length + 1, ascii_letters?, scriptset, special)
  end

  defp continue([head | tail] = list, acc, length, ascii_letters?, scriptset, special) do
    cond do
      ascii_lower?(head) or ascii_upper?(head) ->
        continue(tail, [head | acc], length + 1, ascii_letters?, ss_latin(scriptset), special)

      head == ?_ or ascii_continue?(head) ->
        continue(tail, [head | acc], length + 1, ascii_letters?, scriptset, special)

      head <= 127 ->
        {acc, list, length, ascii_letters?, scriptset, special}

      true ->
        with @bottom <- unicode_start(head),
             @bottom <- unicode_upper(head),
             @bottom <- unicode_continue(head) do
          case normalize_start(head) do
            @bottom ->
              {:error, {:unexpected_token, :lists.reverse([head | acc])}}

            {head, ss} ->
              ss = ss_intersect(scriptset, ss)
              special = [:nfkc | List.delete(special, :nfkc)]
              continue(tail, [head | acc], length + 1, false, ss, special)
          end
        else
          ss ->
            ss = ss_intersect(scriptset, ss)
            continue(tail, [head | acc], length + 1, false, ss, special)
        end
    end
  end

  defp continue([], acc, length, ascii_letters?, scriptset, special) do
    {acc, [], length, ascii_letters?, scriptset, special}
  end

  defp validate({:error, _} = error, _kind) do
    error
  end

  defp validate({acc, rest, length, true, _scriptset, special}, kind) do
    {kind, :lists.reverse(acc), rest, length, true, special}
  end

  defp validate({original_acc, rest, length, false, _scriptset, special}, kind) do
    original_acc = :lists.reverse(original_acc)
    acc = :unicode.characters_to_nfc_list(original_acc)

    special =
      if original_acc == acc do
        special
      else
        [:nfkc | List.delete(special, :nfkc)]
      end

    {kind, acc, rest, length, false, special}
  end
end

defmodule String.Tokenizer.Security do
  @moduledoc false
  # Confusable/bidi lint warnings need the UTS 39 tables; not tracked.
  def unicode_lint_warnings(_tokens), do: []
  def confusable_skeleton(s), do: s
end
