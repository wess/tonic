defmodule Inspect.Opts do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
















































































  # TODO: Remove :char_lists key on v2.0
  defstruct base: :decimal,
            binaries: :infer,
            char_lists: :infer,
            charlists: :infer,
            custom_options: [],
            inspect_fun: &Inspect.inspect/2,
            limit: 50,
            pretty: false,
            printable_limit: 4096,
            safe: true,
            structs: true,
            syntax_colors: [],
            width: 80























  def new(opts) do
    struct(%Inspect.Opts{inspect_fun: default_inspect_fun()}, opts)
  end






  def default_inspect_fun do
    :persistent_term.get({__MODULE__, :inspect_fun}, &Inspect.inspect/2)
  end


































  def default_inspect_fun(fun) when is_function(fun, 2) do
    :persistent_term.put({__MODULE__, :inspect_fun}, fun)
  end
end

defmodule Inspect.Algebra do







































































  @container_separator ","
  @tail_separator " |"
  @newline "\n"
  @next_break_fits :enabled

  # Functional interface to "doc" records

















  defmacrop doc_string(string, length) do
    quote do: {:doc_string, unquote(string), unquote(length)}
  end


  defmacrop doc_limit(doc, limit) do
    quote do: {:doc_limit, unquote(doc), unquote(limit)}
  end


  defmacrop doc_cons(left, right) do
    quote do: {:doc_cons, unquote(left), unquote(right)}
  end


  defmacrop doc_nest(doc, indent, always_or_break) do
    quote do: {:doc_nest, unquote(doc), unquote(indent), unquote(always_or_break)}
  end


  defmacrop doc_break(break, mode) do
    quote do: {:doc_break, unquote(break), unquote(mode)}
  end


  defmacrop doc_group(group, mode) do
    quote do: {:doc_group, unquote(group), unquote(mode)}
  end


  defmacrop doc_fits(group, mode) do
    quote do: {:doc_fits, unquote(group), unquote(mode)}
  end


  defmacrop doc_force(group) do
    quote do: {:doc_force, unquote(group)}
  end


  defmacrop doc_collapse(count) do
    quote do: {:doc_collapse, unquote(count)}
  end


  defmacrop doc_color(doc, color) do
    quote do: {:doc_color, unquote(doc), unquote(color)}
  end

  @docs [
    :doc_break,
    :doc_collapse,
    :doc_color,
    :doc_cons,
    :doc_fits,
    :doc_force,
    :doc_group,
    :doc_nest,
    :doc_string,
    :doc_limit
  ]

  defguard is_doc(doc)
           when is_binary(doc) or doc in [:doc_nil, :doc_line] or
                  (is_tuple(doc) and elem(doc, 0) in @docs)

  defguardp is_limit(limit) when limit == :infinity or (is_integer(limit) and limit >= 0)
  defguardp is_width(limit) when limit == :infinity or (is_integer(limit) and limit >= 0)

  # Elixir + Inspect.Opts conveniences
  # These have the _doc suffix.






  def to_doc(term, opts)

  def to_doc(%_{} = struct, %Inspect.Opts{inspect_fun: fun} = opts) do
    if opts.structs do
      try do
        fun.(struct, opts)
      rescue
        caught_exception ->
          # Because we try to raise a nice error message in case
          # we can't inspect a struct, there is a chance the error
          # message itself relies on the struct being printed, so
          # we need to trap the inspected messages to guarantee
          # we won't try to render any failed instruct when building
          # the error message.
          if Process.get(:inspect_trap) do
            Inspect.Map.inspect(struct, opts)
          else
            try do
              Process.put(:inspect_trap, true)

              inspected_struct =
                struct
                |> Inspect.Map.inspect(%{
                  opts
                  | syntax_colors: [],
                    inspect_fun: Inspect.Opts.default_inspect_fun()
                })
                |> format(opts.width)
                |> IO.iodata_to_binary()

              inspect_error =
                Inspect.Error.exception(
                  exception: caught_exception,
                  stacktrace: __STACKTRACE__,
                  inspected_struct: inspected_struct
                )

              if opts.safe do
                opts = %{opts | inspect_fun: Inspect.Opts.default_inspect_fun()}
                Inspect.inspect(inspect_error, opts)
              else
                reraise(inspect_error, __STACKTRACE__)
              end
            after
              Process.delete(:inspect_trap)
            end
          end
      end
    else
      Inspect.Map.inspect(struct, opts)
    end
  end

  def to_doc(arg, %Inspect.Opts{inspect_fun: fun} = opts) do
    fun.(arg, opts)
  end













































  def container_doc(left, collection, right, inspect_opts, fun, opts \\ [])
      when is_doc(left) and is_list(collection) and is_doc(right) and is_function(fun, 2) and
             is_list(opts) do
    case collection do
      [] ->
        concat(left, right)

      _ ->
        break = Keyword.get(opts, :break, :maybe)
        separator = Keyword.get(opts, :separator, @container_separator)

        {docs, simple?} =
          container_each(collection, inspect_opts.limit, inspect_opts, fun, [], break == :maybe)

        flex? = simple? or break == :flex
        docs = fold(docs, &join(&1, &2, flex?, separator))

        case flex? do
          true -> group(concat(concat(left, nest(docs, 1)), right))
          false -> group(glue(nest(glue(left, "", docs), 2), "", right))
        end
    end
  end

  defp container_each([], _limit, _opts, _fun, acc, simple?) do
    {:lists.reverse(acc), simple?}
  end

  defp container_each(_, 0, _opts, _fun, acc, simple?) do
    {:lists.reverse(["..." | acc]), simple?}
  end

  defp container_each([term | terms], limit, opts, fun, acc, simple?)
       when is_list(terms) and is_limit(limit) do
    new_limit = decrement(limit)
    doc = fun.(term, %{opts | limit: new_limit})
    limit = if doc == :doc_nil, do: limit, else: new_limit
    container_each(terms, limit, opts, fun, [doc | acc], simple? and simple?(doc))
  end

  defp container_each([left | right], limit, opts, fun, acc, simple?) when is_limit(limit) do
    limit = decrement(limit)
    left = fun.(left, %{opts | limit: limit})
    right = fun.(right, %{opts | limit: limit})
    simple? = simple? and simple?(left) and simple?(right)

    doc = join(left, right, simple?, @tail_separator)
    {:lists.reverse([doc | acc]), simple?}
  end

  defp decrement(:infinity), do: :infinity
  defp decrement(counter), do: counter - 1

  defp join(:doc_nil, :doc_nil, _, _), do: :doc_nil
  defp join(left, :doc_nil, _, _), do: left
  defp join(:doc_nil, right, _, _), do: right
  defp join(left, right, true, sep), do: flex_glue(concat(left, sep), right)
  defp join(left, right, false, sep), do: glue(concat(left, sep), right)

  defp simple?(doc_cons(left, right)), do: simple?(left) and simple?(right)
  defp simple?(doc_color(doc, _)), do: simple?(doc)
  defp simple?(doc_string(_, _)), do: true
  defp simple?(:doc_nil), do: true
  defp simple?(other), do: is_binary(other)



  def surround(left, doc, right) when is_doc(left) and is_doc(doc) and is_doc(right) do
    concat(concat(left, nest(doc, 1)), right)
  end



  def surround_many(
        left,
        docs,
        right,
        %Inspect.Opts{} = inspect,
        fun,
        separator \\ @container_separator
      )
      when is_doc(left) and is_list(docs) and is_doc(right) and is_function(fun, 2) do
    container_doc(left, docs, right, inspect, fun, separator: separator)
  end

  # TODO: Deprecate me on Elixir v1.23

  def color(doc, key, opts) do
    color_doc(doc, key, opts)
  end






  def color_doc(doc, color_key, %Inspect.Opts{syntax_colors: syntax_colors}) when is_doc(doc) do
    if precolor = Keyword.get(syntax_colors, color_key) do
      postcolor = Keyword.get(syntax_colors, :reset, :reset)
      concat(doc_color(doc, ansi(precolor)), doc_color(empty(), ansi(postcolor)))
    else
      doc
    end
  end

  defp ansi(color) do
    color
    |> IO.ANSI.format_fragment(true)
    |> IO.iodata_to_binary()
  end

  # Algebra API











  def empty, do: :doc_nil































  def string(string) when is_binary(string) do
    doc_string(string, String.length(string))
  end












  def concat(doc1, doc2) when is_doc(doc1) and is_doc(doc2) do
    doc_cons(doc1, doc2)
  end
















  def no_limit(doc) do
    doc_limit(doc, :infinity)
  end












  def concat(docs) when is_list(docs) do
    fold(docs, &concat(&1, &2))
  end






  def color(doc, color) when is_doc(doc) and is_binary(color) do
    doc_color(doc, color)
  end






















  def nest(doc, level, mode \\ :always)

  def nest(doc, :cursor, mode) when is_doc(doc) and mode in [:always, :break] do
    doc_nest(doc, :cursor, mode)
  end

  def nest(doc, :reset, mode) when is_doc(doc) and mode in [:always, :break] do
    doc_nest(doc, :reset, mode)
  end

  def nest(doc, 0, _mode) when is_doc(doc) do
    doc
  end

  def nest(doc, level, mode)
      when is_doc(doc) and is_integer(level) and level > 0 and mode in [:always, :break] do
    doc_nest(doc, level, mode)
  end



























  def break(string \\ " ") when is_binary(string) do
    doc_break(string, :strict)
  end







  def collapse_lines(max) when is_integer(max) and max > 0 do
    doc_collapse(max)
  end












































  def next_break_fits(doc, mode \\ @next_break_fits)
      when is_doc(doc) and mode in [:enabled, :disabled] do
    doc_fits(doc, mode)
  end






  def force_unfit(doc) when is_doc(doc) do
    doc_force(doc)
  end































  def flex_break(string \\ " ") when is_binary(string) do
    doc_break(string, :flex)
  end










  def flex_glue(doc1, break_string \\ " ", doc2) when is_binary(break_string) do
    concat(doc1, concat(flex_break(break_string), doc2))
  end



















  def glue(doc1, break_string \\ " ", doc2) when is_binary(break_string) do
    concat(doc1, concat(break(break_string), doc2))
  end





































  def group(doc, mode \\ :self) when is_doc(doc) do
    doc_group(doc, mode)
  end












  def space(doc1, doc2), do: concat(doc1, concat(" ", doc2))






















  def line(), do: :doc_line














  def line(doc1, doc2), do: concat(doc1, concat(line(), doc2))

  # TODO: Deprecate me on Elixir v1.23

  def fold_doc(docs, folder_fun), do: fold(docs, folder_fun)





















  def fold(docs, folder_fun)

  def fold([], _folder_fun), do: empty()
  def fold([doc], _folder_fun), do: doc

  def fold([doc | docs], folder_fun) when is_function(folder_fun, 2),
    do: folder_fun.(doc, fold(docs, folder_fun))





















  def format(doc, width) when is_doc(doc) and is_width(width) do
    format(width, 0, [{0, :flat, doc}])
  end

  # Type representing the document mode to be rendered:
  #
  #   * flat - represents a document with breaks as flats (a break may fit, as it may break)
  #   * break - represents a document with breaks as breaks (a break always fits, since it breaks)
  #   * flat_no_break - represents a document with breaks as flat not allowed to enter in break mode
  #   * break_no_flat - represents a document with breaks as breaks not allowed to enter in flat mode
  #











  # We need at least a break to consider the document does not fit since a
  # large document without breaks has no option but fitting its current line.
  #
  # In case we have groups and the group fits, we need to consider the group
  # parent without the child breaks, hence {:tail, b?, t} below.
  defp fits(w, k, b?, _) when k > w and b?, do: :no_fit
  defp fits(_, _, _, []), do: :fit
  defp fits(w, k, _, {:tail, b?, t}), do: fits(w, k, b?, t)

  ## Flat no break

  defp fits(w, k, b?, [{i, _, doc_fits(x, :disabled)} | t]),
    do: fits(w, k, b?, [{i, :flat_no_break, x} | t])

  defp fits(w, k, b?, [{i, :flat_no_break, doc_fits(x, _)} | t]),
    do: fits(w, k, b?, [{i, :flat_no_break, x} | t])

  ## Breaks no flat

  defp fits(w, k, b?, [{i, _, doc_fits(x, :enabled)} | t]),
    do: fits(w, k, b?, [{i, :break_no_flat, x} | t])

  defp fits(w, k, b?, [{i, :break_no_flat, doc_force(x)} | t]),
    do: fits(w, k, b?, [{i, :break_no_flat, x} | t])

  defp fits(w, k, b?, [{i, :break_no_flat, x} | t])
       when x == :doc_line or (is_tuple(x) and elem(x, 0) == :doc_break) do
    case fits(w, k, b?, [{i, :flat, x} | t]) do
      :no_fit -> :break_next
      fits -> fits
    end
  end

  ## Breaks

  defp fits(_, _, _, [{_, :break, doc_break(_, _)} | _]), do: :fit
  defp fits(_, _, _, [{_, :break, :doc_line} | _]), do: :fit

  defp fits(w, k, b?, [{i, :break, doc_group(x, _)} | t]),
    do: fits(w, k, b?, [{i, :flat, x} | {:tail, b?, t}])

  ## Catch all

  defp fits(w, _, _, [{i, _, :doc_line} | t]), do: fits(w, i, false, t)
  defp fits(w, k, b?, [{_, _, :doc_nil} | t]), do: fits(w, k, b?, t)
  defp fits(w, _, b?, [{i, _, doc_collapse(_)} | t]), do: fits(w, i, b?, t)
  defp fits(w, k, b?, [{i, m, doc_color(x, _)} | t]), do: fits(w, k, b?, [{i, m, x} | t])
  defp fits(w, k, b?, [{_, _, doc_string(_, l)} | t]), do: fits(w, k + l, b?, t)
  defp fits(w, k, b?, [{_, _, s} | t]) when is_binary(s), do: fits(w, k + byte_size(s), b?, t)
  defp fits(_, _, _, [{_, _, doc_force(_)} | _]), do: :no_fit
  defp fits(w, k, _, [{_, _, doc_break(s, _)} | t]), do: fits(w, k + byte_size(s), true, t)
  defp fits(w, k, b?, [{i, m, doc_nest(x, _, :break)} | t]), do: fits(w, k, b?, [{i, m, x} | t])

  defp fits(w, k, b?, [{i, m, doc_nest(x, j, _)} | t]),
    do: fits(w, k, b?, [{apply_nesting(i, k, j), m, x} | t])

  defp fits(w, k, b?, [{i, m, doc_cons(x, y)} | t]),
    do: fits(w, k, b?, [{i, m, x}, {i, m, y} | t])

  defp fits(w, k, b?, [{i, m, doc_group(x, _)} | t]),
    do: fits(w, k, b?, [{i, m, x} | {:tail, b?, t}])

  defp fits(w, k, b?, [{i, m, doc_limit(x, :infinity)} | t]) when w != :infinity,
    do: fits(:infinity, k, b?, [{i, :flat, x}, {i, m, doc_limit(empty(), w)} | t])

  defp fits(_w, k, b?, [{i, m, doc_limit(x, w)} | t]),
    do: fits(w, k, b?, [{i, m, x} | t])






  defp format(_, _, []), do: []
  defp format(w, k, [{_, _, :doc_nil} | t]), do: format(w, k, t)
  defp format(w, _, [{i, _, :doc_line} | t]), do: [indent(i) | format(w, i, t)]
  defp format(w, k, [{i, m, doc_cons(x, y)} | t]), do: format(w, k, [{i, m, x}, {i, m, y} | t])
  defp format(w, k, [{i, m, doc_color(x, c)} | t]), do: [c | format(w, k, [{i, m, x} | t])]
  defp format(w, k, [{_, _, doc_string(s, l)} | t]), do: [s | format(w, k + l, t)]
  defp format(w, k, [{_, _, s} | t]) when is_binary(s), do: [s | format(w, k + byte_size(s), t)]
  defp format(w, k, [{i, m, doc_force(x)} | t]), do: format(w, k, [{i, m, x} | t])

  defp format(w, k, [{i, :flat_no_break, doc_fits(x, :enabled)} | t]),
    do: format(w, k, [{i, :break_no_flat, x} | t])

  defp format(w, k, [{i, m, doc_fits(x, _)} | t]), do: format(w, k, [{i, m, x} | t])
  defp format(w, _, [{i, _, doc_collapse(max)} | t]), do: collapse(format(w, i, t), max, 0, i)

  # Flex breaks are not conditional to the mode
  defp format(w, k, [{i, m, doc_break(s, :flex)} | t]) do
    k = k + byte_size(s)

    if w == :infinity or m == :flat or fits(w, k, true, t) != :no_fit do
      [s | format(w, k, t)]
    else
      [indent(i) | format(w, i, t)]
    end
  end

  # Strict breaks are conditional to the mode
  defp format(w, k, [{i, mode, doc_break(s, :strict)} | t]) do
    if mode == :break do
      [indent(i) | format(w, i, t)]
    else
      [s | format(w, k + byte_size(s), t)]
    end
  end

  # Nesting is conditional to the mode.
  defp format(w, k, [{i, mode, doc_nest(x, j, nest)} | t]) do
    if nest == :always or (nest == :break and mode == :break) do
      format(w, k, [{apply_nesting(i, k, j), mode, x} | t])
    else
      format(w, k, [{i, mode, x} | t])
    end
  end

  # Groups must do the fitting decision.
  defp format(w, k, [{i, :break, doc_group(x, :inherit)} | t]) do
    format(w, k, [{i, :break, x} | t])
  end

  defp format(w, k, [{i, :break_no_flat, doc_group(x, _)} | t]) do
    format(w, k, [{i, :break, x} | t])
  end

  defp format(w, k, [{i, _, doc_group(x, _)} | t]) do
    fits = if w == :infinity, do: :fit, else: fits(w, k, false, [{i, :flat, x}])

    case fits do
      :fit -> format(w, k, [{i, :flat, x} | t])
      :no_fit -> format(w, k, [{i, :break, x} | t])
      :break_next -> format(w, k, [{i, :flat_no_break, x} | t])
    end
  end

  # Limit is set to infinity and then reverts
  defp format(w, k, [{i, m, doc_limit(x, :infinity)} | t]) when w != :infinity do
    format(:infinity, k, [{i, :flat, x}, {i, m, doc_limit(empty(), w)} | t])
  end

  defp format(_w, k, [{i, m, doc_limit(x, w)} | t]) do
    format(w, k, [{i, m, x} | t])
  end

  defp collapse(["\n" <> _ | t], max, count, i) do
    collapse(t, max, count + 1, i)
  end

  defp collapse(["" | t], max, count, i) do
    collapse(t, max, count, i)
  end

  defp collapse(t, max, count, i) do
    [:binary.copy("\n", min(max, count)) <> :binary.copy(" ", i) | t]
  end

  defp apply_nesting(_, k, :cursor), do: k
  defp apply_nesting(_, _, :reset), do: 0
  defp apply_nesting(i, _, j), do: i + j

  defp indent(0), do: @newline
  defp indent(i), do: @newline <> :binary.copy(" ", i)
end

# Imported from Elixir 1.18.3 lib/elixir/lib/inspect/algebra.ex (docs and specs stripped;
# line numbers match the original).
