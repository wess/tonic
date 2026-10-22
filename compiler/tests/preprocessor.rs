#[path = "../src/erl_lex.rs"]
mod erl_lex;
#[allow(dead_code)]
#[path = "../src/erl_parse.rs"]
mod erl_parse;
#[path = "../src/erl_pp.rs"]
mod erl_pp;

use std::fs;
use std::process::Command;

fn preprocess(source: &str) -> Result<Vec<erl_parse::Form>, String> {
    let includes = |_: &str| None;
    let tokens = erl_pp::Pp::new("condition.erl", &includes).run(source)?;
    erl_parse::Parser::new(tokens, "condition.erl").forms()
}

#[test]
fn expressions_and_defined_macros_match_otp_epp() {
    let conditions = [
        "defined(VALUE) andalso defined(BEAM) andalso not defined(MISSING)",
        "(?VALUE + 3) * 2 =:= 20 andalso ((1 bsl 6) bor 3) =:= 67",
        "is_integer(?VALUE) andalso abs(-3) =:= 3 andalso (?VALUE div 2) =:= 3",
        "(10 rem 3) =:= 1 andalso (bnot 0 band 255) =:= 255",
        "1 == 1.0 andalso 1 =/= 1.0 andalso 9007199254740993 > 9007199254740992.0",
        "is_atom(foo) andalso atom > 999999 andalso is_boolean(true)",
        "length([1,2,3]) =:= 3 andalso tuple_size({1,2}) =:= 2 andalso element(2, {a,b}) =:= b",
        "round(1.5) =:= 2 andalso trunc(-1.5) =:= -1 andalso float(2) =:= 2.0",
        "(1 bsl 200) > (1 bsl 199)",
        "false andalso (1 div 0 =:= 2)",
        "true orelse (1 div 0 =:= 2)",
        "(1 div 0) =:= 1",
        "false",
        "7",
    ];
    let dir = std::env::temp_dir().join(format!("tonicepp{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (i, condition) in conditions.iter().enumerate() {
        let source = format!("-module(condition).\n-define(VALUE, 7).\n-if({condition}).\n-selected(true).\n-else.\n-selected(false).\n-endif.\n");
        let file = dir.join("condition.erl");
        fs::write(&file, &source).unwrap();
        let reference = Command::new("erl")
            .args(["-noshell", "-eval", "{ok,F} = epp:parse_file(hd(init:get_plain_arguments()), [], []), io:format(\"~p~n\", [[V || {attribute,_,selected,V} <- F]]), halt().", "-extra"])
            .arg(&file).output().unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let tokens = erl_pp::Pp::new("condition.erl", &|_| None)
            .run(&source)
            .unwrap();
        let got = tokens
            .windows(3)
            .find_map(|ts| match (&ts[0].t, &ts[1].t, &ts[2].t) {
                (erl_lex::T::Atom(n), erl_lex::T::P(p), erl_lex::T::Atom(v))
                    if n == "selected" && p == "(" =>
                {
                    Some(v.clone())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            String::from_utf8(reference.stdout).unwrap(),
            format!("[{got}]\n"),
            "condition {i}: {condition}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unsupported_expressions_and_unbalanced_directives_are_errors() {
    for condition in [
        "unknown(1)",
        "false andalso unknown(1)",
        "X = 1",
        "begin true end",
    ] {
        let source = format!("-module(condition).\n-if({condition}).\n-selected(true).\n-endif.\n");
        assert!(preprocess(&source).is_err(), "accepted {condition}");
    }
    for source in [
        "-module(condition).\n-endif.\n",
        "-module(condition).\n-if(true).\n",
        "-module(condition).\n-if(true).\n-else.\n-else.\n-endif.\n",
        "-module(condition).\n-if(true).\n-else.\n-elif(true).\n-endif.\n",
        "-define(VALUE, 1).\n-define(VALUE, 2).\n",
    ] {
        assert!(preprocess(source).is_err(), "accepted {source}");
    }
    assert!(preprocess("-module(condition).\n-if(false).\n-if(?UNDEFINED).\n-elif(?ALSO_UNDEFINED).\n-else.\n-endif.\n-endif.\n").is_ok());
    assert!(preprocess("-undef(BEAM).\n-ifdef(BEAM).\n-error(bad).\n-endif.\n").is_ok());
}
