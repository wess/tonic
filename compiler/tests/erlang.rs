#[path = "support/process.rs"]
mod process;

use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn map_comprehensions_and_map_generators_compile() {
    let root = std::env::temp_dir().join(format!("tonicerlang{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let module = root.join("comprehensions.erl");
    fs::write(&module, r#"-module(comprehensions).
-export([run/0]).
run() ->
    Doubled = #{K => K * 2 || K <- [1, 2, 3], K =/= 2},
    Pairs = [{K, V} || K := V <- Doubled],
    Swapped = #{V => K || K := V <- Doubled},
    {Doubled, lists:sort(Pairs), Swapped, #{}}.
"#).unwrap();
    let script = root.join("main.exs");
    fs::write(&script, "IO.inspect(:comprehensions.run())\n").unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", module.to_str().unwrap(), script.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(
        String::from_utf8(native.stdout).unwrap(),
        "{%{1 => 2, 3 => 6}, [{1, 2}, {3, 6}], %{2 => 1, 6 => 3}, %{}}\n"
    );
    fs::remove_dir_all(root).unwrap();
}
