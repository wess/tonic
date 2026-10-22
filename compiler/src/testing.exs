defmodule Tonic.TestBridge do
  @switches [
    include: :keep,
    exclude: :keep,
    only: :keep,
    seed: :integer,
    max_cases: :integer,
    max_failures: :integer,
    timeout: :integer,
    trace: :boolean,
    color: :boolean,
    slowest: :integer,
    slowest_modules: :integer,
    exit_status: :integer,
    repeat_until_failure: :integer,
    start: :boolean
  ]

  def options(args) do
    {opts, selectors, invalid} = OptionParser.parse(args, strict: @switches)

    if invalid != [],
      do: Mix.raise("unsupported native test options: #{inspect(invalid)}")

    {parsed, locations} = ExUnit.Filters.parse_paths(selectors)

    {config, _allowed} =
      Mix.Tasks.Test.process_ex_unit_opts(
        Keyword.put(
          opts,
          :failures_manifest_path,
          Path.join(Process.get(:tonic_project_output), "testfailures")
        )
      )

    %{
      config: config,
      locations: locations,
      selectors: parsed,
      only: Keyword.has_key?(opts, :only),
      start: Keyword.get(opts, :start, true)
    }
  end

  def files(config, selectors, project_root) do
    test_paths = config[:test_paths] || ["test"]
    pattern = config[:test_pattern] || "*_test.exs"

    paths =
      if selectors == [] do
        test_paths
      else
        Enum.map(selectors, fn path ->
          from_root = Path.expand(path, project_root)
          if File.exists?(from_root), do: from_root, else: Path.expand(path)
        end)
      end

    files = Mix.Utils.extract_files(paths, pattern) |> Enum.map(&Path.expand/1) |> Enum.sort()

    files =
      if Path.expand(File.cwd!()) == Path.expand(project_root) do
        files
      else
        Enum.filter(files, &String.starts_with?(&1, Path.expand(File.cwd!()) <> "/"))
      end

    helpers =
      if selectors == [] or files != [] do
        Enum.map(test_paths, &Path.expand(Path.join(&1, "test_helper.exs")))
        |> Enum.filter(&File.regular?/1)
      else
        []
      end

    helpers ++ files
  end

  def runner(options) do
    """
    tonic_test_options = #{inspect(options.config, limit: :infinity)}
    tonic_helper_excludes = List.wrap(Application.get_env(:ex_unit, :exclude, []))
    tonic_test_options = Keyword.update(tonic_test_options, :exclude, tonic_helper_excludes, &Enum.uniq(&1 ++ tonic_helper_excludes))
    ExUnit.configure(Keyword.merge(tonic_test_options, #{inspect(options.locations, limit: :infinity)}))
    %{failures: failures, excluded: excluded, total: total} = ExUnit.run()
    cond do
      failures > 0 -> System.halt(#{Keyword.fetch!(options.config, :exit_status)})
      #{options.only} and excluded == total ->
        IO.puts(:stderr, #{inspect(~s(The --only option was given to "mix test" but no test was executed))})
        System.halt(1)
      true -> System.halt(0)
    end
    """
  end
end
