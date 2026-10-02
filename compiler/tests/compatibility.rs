use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn check(source: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!(
        "toniccompat{}{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("source.exs");
    fs::write(&file, source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_tonic"))
        .arg("check")
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_dir_all(dir).unwrap();
    result
}

#[test]
fn chained_overrides_keep_previous_definitions_and_defaults() {
    let out = check(
        r#"
defmodule Override do
  def value(x \\ 2), do: x + 1
  defoverridable value: 1
  def value(x), do: super(x) * 2
  defoverridable value: 1
  def value(x), do: super(x) + 5
  def zero(), do: 3
  defoverridable zero: 0
  def zero(), do: super() + 1
end
IO.inspect({Override.value(), Override.value(4), Override.zero()})
"#,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("undefined"));
}

#[test]
fn super_requires_an_overridden_definition_and_matching_arity() {
    let missing = check("defmodule Missing do\n def value(), do: super()\nend\n");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("super is not allowed here"));
    let wrong = check("defmodule Wrong do\n def value(x), do: x\n defoverridable value: 1\n def value(x), do: super()\nend\n");
    assert!(!wrong.status.success());
    assert!(
        String::from_utf8_lossy(&wrong.stderr).contains("super must be called with 1 arguments")
    );
}

#[test]
fn making_an_undefined_function_overridable_fails() {
    let out = check("defmodule Undefined do\n defoverridable missing: 1\nend\n");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("was not defined"));
}

#[test]
fn crossfile_callbacks_and_caller_macros_execute() {
    let dir = std::env::temp_dir().join(format!(
        "toniccallbacks{}{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let hooks = dir.join("hooks.ex");
    fs::write(
        &hooks,
        r#"
defmodule CrossHooks do
  def __on_definition__(env, _kind, name, _args, _guards, _body) do
    Module.put_attribute(env.module, :last, name)
  end
  defmacro __before_compile__(env) do
    last = Module.get_attribute(env.module, :last)
    Module.put_attribute(env.module, :value, 42)
    quote do
      def injected(), do: {@value, unquote(last)}
    end
  end
  defmacro caller(), do: quote(do: unquote(__CALLER__.module))
end
"#,
    )
    .unwrap();
    let consumer = dir.join("consumer.exs");
    fs::write(
        &consumer,
        r#"
defmodule CrossConsumer do
  require CrossHooks
  @on_definition CrossHooks
  @before_compile CrossHooks
  def original(), do: CrossHooks.caller()
  def recorded(), do: @last
end
IO.inspect({CrossConsumer.original(), CrossConsumer.recorded(), CrossConsumer.injected()})
"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tonic"))
        .arg("run")
        .arg(&hooks)
        .arg(&consumer)
        .output()
        .unwrap();
    fs::remove_dir_all(dir).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "{CrossConsumer, :original, {42, :recorded}}\n"
    );
}

#[test]
fn behaviour_overrides_only_declared_callbacks() {
    let out = check(
        r#"
defmodule OverrideBehaviour do
  @callback value(integer()) :: integer()
  @callback missing() :: integer()
end
defmodule BehaviourImplementation do
  @behaviour OverrideBehaviour
  def value(x), do: x
  def unrelated(), do: 1
  defoverridable OverrideBehaviour
  def value(x), do: super(x) + 1
end
"#,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let unrelated = check(
        r#"
defmodule RelatedBehaviour do
  @callback value(integer()) :: integer()
end
defmodule UnrelatedImplementation do
  def value(x), do: x
  def unrelated(), do: 1
  defoverridable RelatedBehaviour
  def unrelated(), do: super()
end
"#,
    );
    assert!(!unrelated.status.success());
    assert!(String::from_utf8_lossy(&unrelated.stderr).contains("super is not allowed here"));
}
