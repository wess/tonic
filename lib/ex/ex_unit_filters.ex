defmodule ExUnit.Filters do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.


  alias ExUnit.FailuresManifest













  # TODO: Deprecate this on Elixir v1.20
  def parse_path(file_path) do
    {[parsed_path], ex_unit_opts} = parse_paths([file_path])
    {parsed_path, ex_unit_opts}
  end







  def parse_paths(file_paths) do
    {parsed_paths, locations} =
      Enum.map_reduce(file_paths, [], fn file_path, locations ->
        case extract_line_numbers(file_path) do
          {path, []} -> {path, locations}
          {path, lines} -> {path, [{:location, {path, lines}} | locations]}
        end
      end)

    ex_unit_opts =
      if locations == [], do: [], else: [exclude: [:test], include: Enum.reverse(locations)]

    {parsed_paths, ex_unit_opts}
  end

  defp extract_line_numbers(file_path) do
    case Path.relative_to_cwd(file_path) |> String.split(":") do
      [path] ->
        {path, []}

      [path | parts] ->
        {path_parts, line_numbers} = Enum.split_while(parts, &(to_line_number(&1) == nil))
        path = Enum.join([path | path_parts], ":") |> Path.split() |> Path.join()
        lines = for n <- line_numbers, valid_number = validate_line_number(n), do: valid_number

        case lines do
          [line] -> {path, line}
          lines -> {path, lines}
        end
    end
  end

  defp to_line_number(str) do
    case Integer.parse(str) do
      {x, ""} when x > 0 -> x
      _ -> nil
    end
  end

  defp validate_line_number(str) do
    number = to_line_number(str)
    number == nil && IO.warn("invalid line number given as ExUnit filter: #{str}", [])
    number
  end

































  def normalize(include, exclude) do
    {include_atoms, include_tags} =
      include |> List.wrap() |> Enum.uniq() |> Enum.split_with(&is_atom/1)

    {exclude_atoms, exclude_tags} =
      exclude |> List.wrap() |> Enum.uniq() |> Enum.split_with(&is_atom/1)

    exclude_tags_map = Map.new(exclude_tags)

    exclude_included =
      for include_tag <- include_tags, key = has_tag(include_tag, exclude_tags_map), do: key

    exclude_tags = exclude_tags |> Keyword.drop(include_atoms) |> Keyword.drop(exclude_included)

    {include_atoms ++ include_tags, (exclude_atoms -- include_atoms) ++ exclude_tags}
  end











  def parse(filters) do
    Enum.map(filters, fn filter ->
      case :binary.split(filter, ":") do
        [key, value] -> parse_kv(String.to_atom(key), value)
        [key] -> String.to_atom(key)
      end
    end)
  end

  defp parse_kv(:line, line) when is_binary(line), do: {:line, String.to_integer(line)}
  defp parse_kv(:location, loc) when is_binary(loc), do: {:location, extract_line_numbers(loc)}
  defp parse_kv(key, value), do: {key, value}














  def failure_info(manifest_file) do
    FailuresManifest.info(manifest_file)
  end








  def fail_all!(manifest_file) do
    FailuresManifest.fail_all!(manifest_file)
  end


































  def eval(include, exclude, tags, collection) when is_map(tags) do
    cond do
      Enum.any?(include, &has_tag(&1, tags, collection)) ->
        maybe_skipped(include, tags, collection)

      excluded = Enum.find_value(exclude, &has_tag(&1, tags, collection)) ->
        {:excluded, "due to #{excluded} filter"}

      true ->
        maybe_skipped(include, tags, collection)
    end
  end

  defp maybe_skipped(include, tags, collection) do
    case tags do
      %{skip: skip} when is_binary(skip) or skip == true ->
        skip_tags = %{skip: skip}
        skip_included_explicitly? = Enum.any?(include, &has_tag(&1, skip_tags, collection))

        cond do
          skip_included_explicitly? -> :ok
          is_binary(skip) -> {:skipped, skip}
          skip -> {:skipped, "due to skip tag"}
        end

      _ ->
        :ok
    end
  end

  defp has_tag({:location, {path, lines}}, %{line: _, describe_line: _} = tags, collection) do
    String.ends_with?(tags.file, path) and
      lines |> List.wrap() |> Enum.any?(&has_tag({:line, &1}, tags, collection))
  end

  defp has_tag({:line, line}, %{line: _, describe_line: _} = tags, collection)
       when is_integer(line) do
    cond do
      tags.describe_line == line ->
        true

      describe_block?(line, collection) ->
        false

      true ->
        tags.line <= line and closest_test_before_line(line, collection).tags.line == tags.line
    end
  end

  defp has_tag(pair, tags, _collection) do
    has_tag(pair, tags)
  end

  defp has_tag({key, %Regex{} = value}, tags) when is_atom(key) do
    case Map.fetch(tags, key) do
      {:ok, tag} -> to_string(tag) =~ value and key
      _ -> false
    end
  end

  defp has_tag({key, value}, tags) when is_atom(key) do
    case Map.fetch(tags, key) do
      {:ok, ^value} -> key
      {:ok, tag} -> compare(to_string(tag), to_string(value)) and key
      _ -> false
    end
  end

  defp has_tag(key, tags) when is_atom(key), do: Map.has_key?(tags, key) and key

  defp compare("Elixir." <> tag1, tag2), do: compare(tag1, tag2)
  defp compare(tag1, "Elixir." <> tag2), do: compare(tag1, tag2)
  defp compare(tag, tag), do: true
  defp compare(_, _), do: false

  defp describe_block?(line, collection) do
    Enum.any?(collection, fn %ExUnit.Test{tags: %{describe_line: describe_line}} ->
      line == describe_line
    end)
  end

  defp closest_test_before_line(line, collection) do
    Enum.min_by(collection, fn %ExUnit.Test{tags: %{line: test_line}} ->
      if line - test_line >= 0 do
        line - test_line
      else
        :infinity
      end
    end)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../ex_unit/ex_unit/filters.ex (docs and specs stripped;
# line numbers match the original).
