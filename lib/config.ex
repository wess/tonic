# Mix evaluates compile-time configuration; native programs apply those values
# before evaluating config/runtime.exs and starting applications.
defmodule Config do
  @moduledoc false

  defmacro __using__(_opts) do
    quote do
      import Config, only: [config: 2, config: 3, config_env: 0, config_target: 0, import_config: 1]
    end
  end

  def config(root_key, opts) when is_atom(root_key) and is_list(opts) do
    unless Keyword.keyword?(opts) do
      raise ArgumentError, "config/2 expected a keyword list, got: #{inspect(opts)}"
    end

    Enum.each(opts, fn {k, v} -> put(root_key, k, v) end)
  end

  def config(root_key, key, opts) when is_atom(root_key) and is_atom(key) do
    put(root_key, key, opts)
  end

  defp put(app, key, value) do
    value =
      case :application.get_env(app, key) do
        {:ok, old} -> deep_merge(key, old, value)
        :undefined -> value
      end

    :application.set_env(app, key, value)
  end

  defp deep_merge(_key, value1, value2) do
    if Keyword.keyword?(value1) and Keyword.keyword?(value2) do
      Keyword.merge(value1, value2, &deep_merge/3)
    else
      value2
    end
  end

  def config_env, do: :application.get_env(:tonic_config, :env, :dev)
  def config_target, do: :application.get_env(:tonic_config, :target, :host)

  def import_config(_file), do: raise(ArgumentError, "import_config is only supported in Mix compile-time configuration")
end

defmodule Mix do
  @moduledoc false
  # The bits of Mix that project code commonly calls at runtime.
  def env, do: Config.config_env()
  def target, do: Config.config_target()
end
