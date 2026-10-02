%% Parsing entry points of Elixir's elixir.erl (string_to_tokens,
%% tokens_to_quoted, string_to_quoted!), copied from Elixir 1.18.3.
-module(elixir).
-export([string_to_tokens/5, tokens_to_quoted/3, 'string_to_quoted!'/5]).
%% Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
string_to_tokens(String, StartLine, StartColumn, File, Opts) when is_integer(StartLine), is_binary(File) ->
  case elixir_tokenizer:tokenize(String, StartLine, StartColumn, Opts) of
    {ok, _Line, _Column, [], Tokens, []} ->
      {ok, lists:reverse(Tokens)};
    {ok, _Line, _Column, Warnings, Tokens, Terminators} ->
      (lists:keyfind(emit_warnings, 1, Opts) /= {emit_warnings, false}) andalso
        [elixir_errors:erl_warn(L, File, M) || {L, M} <- lists:reverse(Warnings)],
      {ok, lists:reverse(Tokens, Terminators)};
    {error, Info, _Rest, _Warnings, _SoFar} ->
      {error, format_token_error(Info)}
  end.

format_token_error({Location, {ErrorPrefix, ErrorSuffix}, Token}) ->
  {Location, {to_binary(ErrorPrefix), to_binary(ErrorSuffix)}, to_binary(Token)};
format_token_error({Location, Error, Token}) ->
  {Location, to_binary(Error), to_binary(Token)}.

tokens_to_quoted(Tokens, WarningFile, Opts) ->
  handle_parsing_opts(WarningFile, Opts),

  try elixir_parser:parse(Tokens) of
    {ok, Forms} ->
      {ok, Forms};
    {error, {Line, _, [{ErrorPrefix, ErrorSuffix}, Token]}} ->
      {error, {parser_location(Line), {to_binary(ErrorPrefix), to_binary(ErrorSuffix)}, to_binary(Token)}};
    {error, {Line, _, [Error, Token]}} ->
      {error, {parser_location(Line), to_binary(Error), to_binary(Token)}}
  after
    erase(elixir_parser_warning_file),
    erase(elixir_parser_columns),
    erase(elixir_token_metadata),
    erase(elixir_literal_encoder)
  end.

parser_location({Line, Column, _}) ->
  [{line, Line}, {column, Column}];
parser_location(Meta) ->
  Line =
    case lists:keyfind(line, 1, Meta) of
      {line, L} -> L;
      false -> 0
    end,

  case lists:keyfind(column, 1, Meta) of
    {column, C} -> [{line, Line}, {column, C}];
    false -> [{line, Line}]
  end.

'string_to_quoted!'(String, StartLine, StartColumn, File, Opts) ->
  case string_to_tokens(String, StartLine, StartColumn, File, Opts) of
    {ok, Tokens} ->
      case tokens_to_quoted(Tokens, File, Opts) of
        {ok, Forms} ->
          Forms;
        {error, {Meta, Error, Token}} ->
          elixir_errors:parse_error(Meta, File, Error, Token, {String, StartLine, StartColumn})
      end;
    {error, {Meta, Error, Token}} ->
      elixir_errors:parse_error(Meta, File, Error, Token, {String, StartLine, StartColumn})
  end.

to_binary(List) when is_list(List) -> elixir_utils:characters_to_binary(List);
to_binary(Atom) when is_atom(Atom) -> atom_to_binary(Atom).

handle_parsing_opts(File, Opts) ->
  WarningFile =
    case lists:keyfind(emit_warnings, 1, Opts) of
      {emit_warnings, false} -> nil;
      _ -> File
    end,
  LiteralEncoder =
    case lists:keyfind(literal_encoder, 1, Opts) of
      {literal_encoder, Fun} -> Fun;
      false -> false
    end,
  TokenMetadata = lists:keyfind(token_metadata, 1, Opts) == {token_metadata, true},
  Columns = lists:keyfind(columns, 1, Opts) == {columns, true},
  put(elixir_parser_warning_file, WarningFile),
  put(elixir_parser_columns, Columns),
  put(elixir_token_metadata, TokenMetadata),
  put(elixir_literal_encoder, LiteralEncoder).
