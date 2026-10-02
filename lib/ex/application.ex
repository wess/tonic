defmodule Application do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.













































































































































































































































































































































































































  @optional_callbacks start_phase: 3, prep_stop: 1, config_change: 3


  defmacro __using__(_) do
    quote location: :keep do
      @behaviour Application


      def stop(_state) do
        :ok
      end

      defoverridable Application
    end
  end

  @application_keys [
    :description,
    :id,
    :vsn,
    :modules,
    :maxP,
    :maxT,
    :registered,
    :included_applications,
    :optional_applications,
    :applications,
    :mod,
    :start_phases
  ]

  application_key_specs = Enum.reduce(@application_keys, &{:|, [], [&1, &2]})













































  def spec(app) when is_atom(app) do
    case :application.get_all_key(app) do
      {:ok, info} -> :lists.keydelete(:env, 1, info)
      :undefined -> nil
    end
  end









  def spec(app, key) when is_atom(app) and key in @application_keys do
    case :application.get_key(app, key) do
      {:ok, value} -> value
      :undefined -> nil
    end
  end









  def get_application(module) when is_atom(module) do
    case :application.get_application(module) do
      {:ok, app} -> app
      :undefined -> nil
    end
  end





  def get_all_env(app) when is_atom(app) do
    :application.get_all_env(app)
  end













































  def compile_env(app, key_or_path, default \\ nil) when is_atom(app) do
    case fetch_compile_env(app, key_or_path, %{tracers: []}) do
      {:ok, value} -> value
      :error -> default
    end
  end



















  def compile_env(%Macro.Env{} = env, app, key_or_path, default) do
    case fetch_compile_env(app, key_or_path, env) do
      {:ok, value} -> value
      :error -> default
    end
  end









  def compile_env!(app, key_or_path) when is_atom(app),
    do: compile_env!(%Macro.Env{}, app, key_or_path)
























  def compile_env!(%Macro.Env{} = env, app, key_or_path) do
    case fetch_compile_env(app, key_or_path, env) do
      {:ok, value} ->
        value

      :error ->
        raise ArgumentError,
              "could not fetch application environment #{inspect(key_or_path)} for application " <>
                "#{inspect(app)} #{fetch_env_failed_reason(app, key_or_path)}"
    end
  end

  defp fetch_compile_env(app, key, env) when is_atom(key) do
    fetch_compile_env(app, key, [], env)
  end

  defp fetch_compile_env(app, [key | paths], env) when is_atom(key),
    do: fetch_compile_env(app, key, paths, env)

  defp fetch_compile_env(app, key, path, env) do
    return = traverse_env(fetch_env(app, key), path)

    for tracer <- env.tracers do
      tracer.trace({:compile_env, app, [key | path], return}, env)
    end

    return
  end

  defp traverse_env(return, []), do: return
  defp traverse_env(:error, _paths), do: :error
  defp traverse_env({:ok, value}, [key | keys]), do: traverse_env(Access.fetch(value, key), keys)



















































  def get_env(app, key, default \\ nil) when is_atom(app) do
    maybe_warn_on_app_env_key(app, key)
    :application.get_env(app, key, default)
  end



















  def fetch_env(app, key) when is_atom(app) do
    maybe_warn_on_app_env_key(app, key)

    case :application.get_env(app, key) do
      {:ok, value} -> {:ok, value}
      :undefined -> :error
    end
  end



















  def fetch_env!(app, key) when is_atom(app) do
    case fetch_env(app, key) do
      {:ok, value} ->
        value

      :error ->
        raise ArgumentError,
              "could not fetch application environment #{inspect(key)} for application " <>
                "#{inspect(app)} #{fetch_env_failed_reason(app, key)}"
    end
  end

  defp fetch_env_failed_reason(app, key) do
    vsn = :application.get_key(app, :vsn)

    case vsn do
      {:ok, _} ->
        "because configuration at #{inspect(key)} was not set"

      :undefined ->
        "because the application was not loaded nor configured"
    end
  end

























  def put_env(app, key, value, opts \\ []) when is_atom(app) do
    maybe_warn_on_app_env_key(app, key)
    :application.set_env(app, key, value, opts)
  end




























  def put_all_env(config, opts \\ []) when is_list(config) and is_list(opts) do
    :application.set_env(config, opts)
  end







  def delete_env(app, key, opts \\ []) when is_atom(app) do
    maybe_warn_on_app_env_key(app, key)
    :application.unset_env(app, key, opts)
  end

  defp maybe_warn_on_app_env_key(_app, key) when is_atom(key),
    do: :ok

  # TODO: Remove this deprecation warning on 2.0+ and allow list lookups as in compile_env.
  defp maybe_warn_on_app_env_key(app, key) do
    message = fn ->
      "passing non-atom as application env key is deprecated, got: #{inspect(key)}"
    end

    IO.warn_once({Application, :key, app, key}, message, _stacktrace_drop_levels = 2)
  end








  def ensure_started(app, type \\ :temporary) when is_atom(app) and is_atom(type) do
    :application.ensure_started(app, type)
  end









  def ensure_loaded(app) when is_atom(app) do
    case :application.load(app) do
      :ok -> :ok
      {:error, {:already_loaded, ^app}} -> :ok
      {:error, _} = error -> error
    end
  end





















  def ensure_all_started(app_or_apps, type_or_opts \\ [])

  def ensure_all_started(app, type) when is_atom(type) do
    ensure_all_started(app, type: type)
  end

  def ensure_all_started(app, opts) when is_atom(app) do
    ensure_all_started([app], opts)
  end



  def ensure_all_started(apps, opts) when is_list(apps) and is_list(opts) do
    opts = Keyword.validate!(opts, type: :temporary, mode: :serial)

    if function_exported?(:application, :ensure_all_started, 3) do
      :application.ensure_all_started(apps, opts[:type], opts[:mode])
    else
      # TODO: Remove this clause when we require Erlang/OTP 26+
      Enum.reduce_while(apps, {:ok, []}, fn app, {:ok, acc} ->
        case :application.ensure_all_started(app, opts[:type]) do
          {:ok, apps} -> {:cont, {:ok, apps ++ acc}}
          {:error, e} -> {:halt, {:error, e}}
        end
      end)
    end
  end
















  def start(app, type \\ :temporary) when is_atom(app) and is_atom(type) do
    :application.start(app, type)
  end







  def stop(app) when is_atom(app) do
    :application.stop(app)
  end











  def load(app) when is_atom(app) do
    :application.load(app)
  end








  def unload(app) when is_atom(app) do
    :application.unload(app)
  end


























  def app_dir(app) when is_atom(app) do
    case :code.lib_dir(app) do
      lib when is_list(lib) -> IO.chardata_to_string(lib)
      {:error, :bad_name} -> raise ArgumentError, "unknown application: #{inspect(app)}"
    end
  end





















  def app_dir(app, path)

  def app_dir(app, path) when is_atom(app) and is_binary(path) do
    Path.join(app_dir(app), path)
  end

  def app_dir(app, path) when is_atom(app) and is_list(path) do
    Path.join([app_dir(app) | path])
  end





  def started_applications(timeout \\ 5000) do
    :application.which_applications(timeout)
  end





  def loaded_applications do
    :application.loaded_applications()
  end







  def format_error(reason) do
    try do
      do_format_error(reason)
    catch
      # A user could create an error that looks like a built-in one
      # causing an error.
      :error, _ ->
        inspect(reason)
    end
  end

  # exit(:normal) call is special cased, undo the special case.
  defp do_format_error({{:EXIT, :normal}, {mod, :start, args}}) do
    Exception.format_exit({:normal, {mod, :start, args}})
  end

  # {:error, reason} return value
  defp do_format_error({reason, {mod, :start, args}}) do
    Exception.format_mfa(mod, :start, args) <>
      " returned an error: " <> Exception.format_exit(reason)
  end

  # error or exit(reason) call, use exit reason as reason.
  defp do_format_error({:bad_return, {{mod, :start, args}, {:EXIT, reason}}}) do
    Exception.format_exit({reason, {mod, :start, args}})
  end

  # bad return value
  defp do_format_error({:bad_return, {{mod, :start, args}, return}}) do
    Exception.format_mfa(mod, :start, args) <> " returned a bad value: " <> inspect(return)
  end

  defp do_format_error({:already_started, app}) when is_atom(app) do
    "already started application #{app}"
  end

  defp do_format_error({:not_started, app}) when is_atom(app) do
    "not started application #{app}"
  end

  defp do_format_error({:bad_application, app}) do
    "bad application: #{inspect(app)}"
  end

  defp do_format_error({:already_loaded, app}) when is_atom(app) do
    "already loaded application #{app}"
  end

  defp do_format_error({:not_loaded, app}) when is_atom(app) do
    "not loaded application #{app}"
  end

  defp do_format_error({:invalid_restart_type, restart}) do
    "invalid application restart type: #{inspect(restart)}"
  end

  defp do_format_error({:invalid_name, name}) do
    "invalid application name: #{inspect(name)}"
  end

  defp do_format_error({:invalid_options, opts}) do
    "invalid application options: #{inspect(opts)}"
  end

  defp do_format_error({:badstartspec, spec}) do
    "bad application start specs: #{inspect(spec)}"
  end

  defp do_format_error({~c"no such file or directory", file}) do
    "could not find application file: #{file}"
  end

  defp do_format_error(reason) do
    Exception.format_exit(reason)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/application.ex (docs and specs stripped;
# line numbers match the original).
