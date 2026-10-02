defmodule ExUnit do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.

























































































  defmodule Test do














    defstruct [:name, :case, :module, :state, time: 0, tags: %{}, logs: "", parameters: %{}]

    # TODO: Remove the `:case` field on v2.0









  end

  defmodule TestModule do




















    defstruct [:file, :name, :setup_all?, :state, parameters: %{}, tags: %{}, tests: []]










  end

  defmodule TestCase do
    # TODO: Remove this module on v2.0 (it has been replaced by TestModule)

    defstruct [:name, :state, tests: []]


  end

  defmodule TimeoutError do










    defexception [:timeout, :type]


    def message(%{timeout: timeout, type: type}) do
      """
      #{type} timed out after #{timeout}ms. You can change the timeout:

        1. per test by setting "@tag timeout: x" (accepts :infinity)
        2. per test module by setting "@moduletag timeout: x" (accepts :infinity)
        3. globally via "ExUnit.start(timeout: x)" configuration
        4. by running "mix test --timeout x" which sets timeout
        5. or by running "mix test --trace" which sets timeout to infinity
           (useful when using IEx.pry/0)

      where "x" is the timeout given as integer in milliseconds (defaults to 60_000).
      """
    end
  end

  use Application


  def start(_type, []) do
    children = [
      ExUnit.Server,
      ExUnit.CaptureServer
    ]

    opts = [strategy: :one_for_one, name: ExUnit.Supervisor]
    Supervisor.start_link(children, opts)
  end












  def start(options \\ []) do
    {:ok, _} = Application.ensure_all_started(:ex_unit)

    configure(options)

    if Application.fetch_env!(:ex_unit, :autorun) do
      Application.put_env(:ex_unit, :autorun, false)

      System.at_exit(fn
        0 ->
          time = ExUnit.Server.modules_loaded(false)
          seed = Application.get_env(:ex_unit, :seed)
          options = persist_defaults(configuration())
          %{failures: failures} = maybe_repeated_run(options, seed, time)

          if failures > 0 do
            System.at_exit(fn _ -> exit({:shutdown, Keyword.fetch!(options, :exit_status)}) end)
          end

        _ ->
          :ok
      end)
    else
      :ok
    end
  end

















































































































  def configure(options) when is_list(options) do
    Enum.each(options, fn {k, v} ->
      Application.put_env(:ex_unit, k, v)
    end)
  end







  def configuration do
    Application.get_all_env(:ex_unit)
    |> put_seed()
    |> put_slowest()
    |> put_max_cases()
  end







  def plural_rule(word) when is_binary(word) do
    Application.get_env(:ex_unit, :plural_rules, %{})
    |> Map.get(word, "#{word}s")
  end







  def plural_rule(word, pluralization) when is_binary(word) and is_binary(pluralization) do
    plural_rules =
      Application.get_env(:ex_unit, :plural_rules, %{})
      |> Map.put(word, pluralization)

    configure(plural_rules: plural_rules)
  end













  def run(additional_modules \\ []) do
    for module <- additional_modules do
      if Code.ensure_loaded?(module) and function_exported?(module, :__ex_unit__, 1) do
        ExUnit.Server.add_module(module, module.__ex_unit__(:config))
      else
        raise(ArgumentError, "#{inspect(module)} is not a ExUnit.Case module")
      end
    end

    _ = ExUnit.Server.modules_loaded(additional_modules != [])
    seed = Application.get_env(:ex_unit, :seed)
    options = persist_defaults(configuration())
    maybe_repeated_run(options, seed, nil)
  end









  def async_run() do
    seed = Application.get_env(:ex_unit, :seed)
    options = persist_defaults(configuration())

    Task.async(fn ->
      maybe_repeated_run(options, seed, nil)
    end)
  end






  def await_run(task) do
    ExUnit.Server.modules_loaded(false)
    Task.await(task, :infinity)
  end













  def after_suite(function) when is_function(function) do
    current_callbacks = Application.fetch_env!(:ex_unit, :after_suite)
    configure(after_suite: [function | current_callbacks])
  end











  def fetch_test_supervisor() do
    case ExUnit.OnExitHandler.get_supervisor(self()) do
      {:ok, nil} ->
        {:ok, sup} = ExUnit.OnExitHandler.Supervisor.start_link([])
        ExUnit.OnExitHandler.put_supervisor(self(), sup)
        {:ok, sup}

      {:ok, _} = ok ->
        ok

      :error ->
        :error
    end
  end

  # Persists default values in application
  # environment before the test suite starts.
  defp persist_defaults(config) do
    config |> Keyword.take([:max_cases, :seed, :trace]) |> configure()
    config
  end

  defp maybe_repeated_run(options, seed, load_us) do
    repeat = Keyword.fetch!(options, :repeat_until_failure)
    maybe_repeated_run(options, seed, load_us, repeat)
  end

  defp maybe_repeated_run(options, seed, load_us, repeat) do
    case ExUnit.Runner.run(options, load_us) do
      {%{failures: 0}, {async_modules, sync_modules}}
      when repeat > 0 and (sync_modules != [] or async_modules != []) ->
        ExUnit.Server.restore_modules(async_modules, sync_modules)

        # Clear the seed if it was generated
        if seed == nil do
          Application.delete_env(:ex_unit, :seed)
        end

        # Re-run configuration
        options = persist_defaults(configuration())
        maybe_repeated_run(options, seed, load_us, repeat - 1)

      {stats, _} ->
        stats
    end
  end

  defp put_seed(opts) do
    Keyword.put_new_lazy(opts, :seed, fn ->
      # We're using `rem System.system_time()` here
      # instead of directly using :os.timestamp or using the
      # :microsecond argument because the VM on Windows has odd
      # precision. Calling with :microsecond will give us a multiple
      # of 1000. Calling without it gives actual microsecond precision.
      System.system_time()
      |> System.convert_time_unit(:native, :microsecond)
      |> rem(1_000_000)
    end)
  end

  defp put_max_cases(opts) do
    Keyword.put(opts, :max_cases, max_cases(opts))
  end

  defp put_slowest(opts) do
    if opts[:slowest] > 0 or opts[:slowest_modules] > 0 do
      Keyword.put(opts, :trace, true)
    else
      opts
    end
  end

  defp max_cases(opts) do
    cond do
      opts[:trace] -> 1
      max = opts[:max_cases] -> max
      true -> System.schedulers_online() * 2
    end
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../ex_unit/ex_unit.ex (docs and specs stripped;
# line numbers match the original).
