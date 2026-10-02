defmodule CompatibilityMacros do
  defmacro caller(), do: quote(do: unquote(__CALLER__.module))
  defmacro answer(x), do: quote(do: unquote(x) + 1)
  defoverridable answer: 1
  defmacro answer(x), do: quote(do: unquote(super(x)) * 2)

  defmacro calculated(value) do
    result = value + 1
    quote do: unquote(result)
  end

  defmacro __using__(opts) do
    value = Keyword.fetch!(opts, :value) + 1
    quote do
      def used(), do: unquote(value)
    end
  end
end

defmodule CompatibilityHooks do
  def track(env, kind, name, args, guards, body) do
    previous = Module.get_attribute(env.module, :definitions) || []
    Module.put_attribute(env.module, :definitions, [{kind, name, length(args), length(guards), body != nil} | previous])
  end

  defmacro inject(env) do
    definitions = Module.get_attribute(env.module, :definitions)
    Module.put_attribute(env.module, :injectedvalue, 17)
    quote do
      def definitions(), do: unquote(Macro.escape(definitions))
      def injected(), do: 42
      def injectedvalue(), do: @injectedvalue
    end
  end

  def completed(env, artifact) do
    File.write!(env.file <> ".compiled", inspect({env.module, is_binary(artifact)}))
  end
end

defmodule CompatibilityConsumer do
  require CompatibilityMacros
  use CompatibilityMacros, value: 8
  def caller(), do: CompatibilityMacros.caller()
  def calculated(), do: CompatibilityMacros.calculated(3)
  def answer(), do: CompatibilityMacros.answer(3)

  def overridden(x \\ 2), do: x + 1
  defoverridable overridden: 1
  def overridden(x), do: super(x) * 2
  defoverridable overridden: 1
  def overridden(x), do: super(x) + 5

  def unchanged(x), do: x + 10
  defoverridable unchanged: 1

  def zero(), do: 1
  defoverridable zero: 0
  def zero(), do: super() + 4
end

defmodule CompatibilityCallbacks do
  @on_definition {CompatibilityHooks, :track}
  @before_compile {CompatibilityHooks, :inject}
  @after_compile {CompatibilityHooks, :completed}
  def original(x), do: x + 1
  def recorded(), do: @definitions
end

IO.inspect({CompatibilityConsumer.caller(), CompatibilityConsumer.calculated(), CompatibilityConsumer.used(), CompatibilityConsumer.answer()})
IO.inspect({CompatibilityConsumer.overridden(), CompatibilityConsumer.overridden(4), CompatibilityConsumer.unchanged(2), CompatibilityConsumer.zero()})
IO.inspect(CompatibilityCallbacks.definitions())
IO.inspect(CompatibilityCallbacks.recorded())
IO.inspect({CompatibilityCallbacks.injected(), CompatibilityCallbacks.injectedvalue()})
path = __ENV__.file <> ".compiled"
IO.puts(File.read!(path))
File.rm!(path)
