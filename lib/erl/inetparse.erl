%% Modified for Tonic; Erlang/OTP 27.3 source/port. Apache-2.0; see licenses/sources.json and notice.
%%
%% %CopyrightBegin%
%%
%% Copyright Ericsson AB 1997-2024. All Rights Reserved.
%%
%% Licensed under the Apache License, Version 2.0 (the "License");
%% you may not use this file except in compliance with the License.
%% You may obtain a copy of the License at
%%
%%     http://www.apache.org/licenses/LICENSE-2.0
%%
%% Unless required by applicable law or agreed to in writing, software
%% distributed under the License is distributed on an "AS IS" BASIS,
%% WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
%% See the License for the specific language governing permissions and
%% limitations under the License.
%%
%% %CopyrightEnd%
%%
-module(inet_parse).
-moduledoc false.

%% Tonic keeps only the address, domain and ntoa parsers; the configuration
%% file parsers depend on kernel file records and are not bundled.

-export([ipv4_address/1, ipv6_address/1]).
-export([ipv4strict_address/1, ipv6strict_address/1]).
-export([address/1, strict_address/1]).
-export([visible_string/1, domain/1]).
-export([ntoa/1, dots/1]).

-import(lists, [reverse/1]).

%% From kernel's inet_int.hrl, the only definitions this section uses.
-define(ip(A,B,C,D),
	(((A) bor (B) bor (C) bor (D)) band (bnot 16#ff)) =:= 0).
-define(ip6(A,B,C,D,E,F,G,H),
	(((A) bor (B) bor (C) bor (D) bor (E) bor (F) bor (G) bor (H))
	 band (bnot 16#ffff)) =:= 0).

%%
%% Check if a String is a string with visible characters #21..#7E
%% visible_string(String) -> Bool
%%
visible_string([C | Cs]) when C >= 16#21, C =< 16#7e -> visible_string(Cs);
visible_string([]) -> true;
visible_string(_) -> false.

%%
%% Check if a String is a domain name according to RFC XXX.
%% domain(String) -> Bool
%%
domain([H|T]) ->
    is_dom1([H|T]);
domain(_) ->
    false.

is_dom1([C | Cs]) when C >= $a, C =< $z -> is_dom_ldh(Cs);
is_dom1([C | Cs]) when C >= $A, C =< $Z -> is_dom_ldh(Cs);
is_dom1([C | Cs]) when C >= $0, C =< $9 -> 
    case is_dom_ldh(Cs) of
	true  -> is_dom2(string:lexemes([C | Cs],"."));
	false -> false
    end;
is_dom1(_) -> false.

is_dom_ldh([C | Cs]) when C >= $a, C =< $z -> is_dom_ldh(Cs);
is_dom_ldh([C | Cs]) when C >= $A, C =< $Z -> is_dom_ldh(Cs);
is_dom_ldh([C | Cs]) when C >= $0, C =< $9 -> is_dom_ldh(Cs);
is_dom_ldh([$-,$. | _]) -> false;
is_dom_ldh([$_,$. | _]) -> false;
is_dom_ldh([$_ | Cs]) -> is_dom_ldh(Cs);
is_dom_ldh([$- | Cs]) -> is_dom_ldh(Cs);
is_dom_ldh([$. | Cs]) -> is_dom1(Cs);
is_dom_ldh([]) -> true;
is_dom_ldh(_) -> false.

%%% Check that we don't get a IP-address as a domain name.

-define(L2I(L), (catch list_to_integer(L))).

is_dom2([A,B,C,D]) ->
    case ?L2I(D) of
	Di when is_integer(Di) ->
	    case {?L2I(A),?L2I(B),?L2I(C)} of
		{Ai,Bi,Ci} when is_integer(Ai),
                                is_integer(Bi),
                                is_integer(Ci) -> false;
		_ -> true
	    end;
	_ -> true
    end;
is_dom2(_) ->
    true.



%%
%% Parse ipv4 address or ipv6 address
%% Return {ok, Address} | {error, Reason}
%%
address(Cs) when is_list(Cs) ->
    case ipv4_address(Cs) of
	{ok,IP} ->
	    {ok,IP};
	_ ->
	    ipv6strict_address(Cs)
    end;
address(_) -> 
    {error, einval}.

%%Parse ipv4 strict address or ipv6 strict address
strict_address(Cs) when is_list(Cs) ->
    case ipv4strict_address(Cs) of
	{ok,IP} ->
	    {ok,IP};
	_ ->
	    ipv6strict_address(Cs)
    end;
strict_address(_) ->
    {error, einval}.

%%
%% Parse IPv4 address: 
%%    d1.d2.d3.d4
%%    d1.d2.d4
%%    d1.d4
%%    d4
%% Any d may be octal, hexadecimal or decimal by C language standards.
%% d4 fills all LSB bytes. This is legacy behaviour from Solaris
%% and FreeBSD. And partly Linux that behave the same except
%% it does not accept hexadecimal.
%%
%% Return {ok, IP} | {error, einval}
%%
ipv4_address(Cs) ->
    try ipv4_addr(Cs) of
	Addr ->
	    {ok,Addr}
    catch
	error:badarg ->
	    {error,einval}
    end.

ipv4_addr(Cs) ->
    case ipv4_addr(Cs, []) of
	[D] when D < (1 bsl 32) ->
	    <<D1,D2,D3,D4>> = <<D:32>>,
	    {D1,D2,D3,D4};
	[D,D1] when D < (1 bsl 24), D1 < 256 ->
	    <<D2,D3,D4>> = <<D:24>>,
	    {D1,D2,D3,D4};
	[D,D2,D1] when D < (1 bsl 16), (D2 bor D1) < 256 ->
	    <<D3,D4>> = <<D:16>>,
	    {D1,D2,D3,D4};
	[D4,D3,D2,D1] when (D4 bor D3 bor D2 bor D1) < 256 ->
	    {D1,D2,D3,D4};
	_ ->
	    erlang:error(badarg)
    end.

ipv4_addr([_|_], [_,_,_,_]) ->
    %% Early bailout for extra characters
    erlang:error(badarg);
ipv4_addr("0x"++Cs, Ds) ->
    ipv4_addr(strip0(Cs), Ds, [], 16, 8);
ipv4_addr("0X"++Cs, Ds) ->
    ipv4_addr(strip0(Cs), Ds, [], 16, 8);
ipv4_addr("0"++Cs, Ds) ->
    ipv4_addr(strip0(Cs), Ds, [$0], 8, 11);
ipv4_addr(Cs, Ds) when is_list(Cs) ->
    ipv4_addr(Cs, Ds, [], 10, 10).

ipv4_addr(Cs0, Ds, Rs, Base, N) ->
    case ipv4_field(Cs0, N, Rs, Base) of
	{D,""} ->
	    [D|Ds];
	{D,[$.|[_|_]=Cs]} ->
	    ipv4_addr(Cs, [D|Ds]);
	{_,_} ->
	    erlang:error(badarg)
    end.

strip0("0"++Cs) ->
    strip0(Cs);
strip0(Cs) when is_list(Cs) ->
    Cs.


%%
%% Parse IPv4 strict dotted decimal address, no leading zeros:
%%    d1.d2.d3.d4
%%
%% Return {ok, IP} | {error, einval}
%%
ipv4strict_address(Cs) ->
    try ipv4strict_addr(Cs) of
	Addr ->
	    {ok,Addr}
    catch
	error:badarg ->
	    {error,einval}
    end.

ipv4strict_addr(Cs) ->
    case ipv4strict_addr(Cs, []) of
	[D4,D3,D2,D1] when (D4 bor D3 bor D2 bor D1) < 256 ->
	    {D1,D2,D3,D4};
	_ ->
	    erlang:error(badarg)
    end.

ipv4strict_addr([_|_], [_,_,_,_]) ->
    %% Early bailout for extra characters
    erlang:error(badarg);
ipv4strict_addr("0", Ds) ->
    [0|Ds];
ipv4strict_addr("0."++Cs, Ds) ->
    ipv4strict_addr(Cs, [0|Ds]);
ipv4strict_addr(Cs0, Ds) when is_list(Cs0) ->
    case ipv4_field(Cs0, 3, [], 10) of
	{D,""} ->
	    [D|Ds];
	{D,[$.|[_|_]=Cs]} ->
	    ipv4strict_addr(Cs, [D|Ds]);
	{_,_} ->
	    erlang:error(badarg)
    end.



ipv4_field("", _, Rs, Base) ->
    {ipv4_field(Rs, Base),""};
ipv4_field("."++_=Cs, _, Rs, Base) ->
    {ipv4_field(Rs, Base),Cs};
ipv4_field("0"++_, _, [], _) ->
    erlang:error(badarg);
ipv4_field([C|Cs], N, Rs, Base) when N > 0 ->
    ipv4_field(Cs, N-1, [C|Rs], Base);
ipv4_field(Cs, _, _, _) when is_list(Cs) ->
    erlang:error(badarg).

ipv4_field(Rs, Base) ->
    V = erlang:list_to_integer(lists:reverse(Rs), Base),
    if  V < 0 ->
	    erlang:error(badarg);
	true ->
	    V
    end.



%%
%% Forgiving IPv6 address
%%
%% Accepts IPv4 address and returns it as a IPv4 compatible IPv6 address
%%
ipv6_address(Cs) ->
    case ipv4_address(Cs) of
	{ok,{D1,D2,D3,D4}} ->
	    {ok,{0,0,0,0,0,16#ffff,(D1 bsl 8) bor D2,(D3 bsl 8) bor D4}};
	_ ->
	    ipv6strict_address(Cs)
    end.

%%
%% Parse IPv6 address according to RFC 4291:
%%     x1:x2:x3:x4:x5:x6:x7:x8
%%     x1:x2::x7:x8
%%     ::x7:x8
%%     x1:x2::
%%     ::
%%     x1:x2:x3:x4:x5:x6:d7a.d7b.d8a.d8b
%%     x1:x2::x5:x6:d7a.d7b.d8a.d8b
%%     ::x5:x6:d7a.d7b.d8a.d8b
%%     x1:x2::d7a.d7b.d8a.d8b
%%     ::d7a.d7b.d8a.d8b
%%     etc
%%
%% Return {ok, IP} | {error, einval}
%%
ipv6strict_address(Cs) ->
    try ipv6_addr(Cs) of
	Addr ->
	    {ok,Addr}
    catch
	error:badarg ->
	    {error,einval}
    end.

ipv6_addr("::") ->
    ipv6_addr_done([], [], 0);
ipv6_addr("::"++Cs) ->
    ipv6_addr(hex(Cs), [], [], 0);
ipv6_addr(Cs) ->
    ipv6_addr(hex(Cs), [], 0).

%% Before "::"
ipv6_addr({Cs0,"%"++Cs1}, A, N) when N == 7 ->
    ipv6_addr_scope(Cs1, [hex_to_int(Cs0)|A], [], N+1, []);
ipv6_addr({Cs0,[]}, A, N) when N == 7 ->
    ipv6_addr_done([hex_to_int(Cs0)|A]);
ipv6_addr({Cs0,"::%"++Cs1}, A, N) when N =< 6 ->
    ipv6_addr_scope(Cs1, [hex_to_int(Cs0)|A], [], N+1, []);
ipv6_addr({Cs0,"::"}, A, N) when N =< 6 ->
    ipv6_addr_done([hex_to_int(Cs0)|A], [], N+1);
ipv6_addr({Cs0,"::"++Cs1}, A, N) when N =< 5 ->
    ipv6_addr(hex(Cs1), [hex_to_int(Cs0)|A], [], N+1);
ipv6_addr({Cs0,":"++Cs1}, A, N) when N =< 6 ->
    ipv6_addr(hex(Cs1), [hex_to_int(Cs0)|A], N+1);
ipv6_addr({Cs0,"."++_=Cs1}, A, N) when N == 6 ->
    ipv6_addr_done(A, [], N, ipv4strict_addr(Cs0++Cs1));
ipv6_addr(_, _, _) ->
    erlang:error(badarg).

%% After "::"
ipv6_addr({Cs0,"%"++Cs1}, A, B, N) when N =< 6 ->
    ipv6_addr_scope(Cs1, A, [hex_to_int(Cs0)|B], N+1, []);
ipv6_addr({Cs0,[]}, A, B, N) when N =< 6 ->
    ipv6_addr_done(A, [hex_to_int(Cs0)|B], N+1);
ipv6_addr({Cs0,":"++Cs1}, A, B, N) when N =< 5 ->
    ipv6_addr(hex(Cs1), A, [hex_to_int(Cs0)|B], N+1);
ipv6_addr({Cs0,"."++_=Cs1}, A, B, N) when N =< 5 ->
    ipv6_addr_done(A, B, N, ipv4strict_addr(Cs0++Cs1));
ipv6_addr(_, _, _, _) ->
    erlang:error(badarg).

%% After "%"
ipv6_addr_scope([], Ar, Br, N, Sr) ->
    ScopeId =
        case lists:reverse(Sr) of
            %% Empty scope id
            "" -> 0;
            %% Scope id starts with 0
            "0"++S -> dec16(S);
            _ -> 0
        end,
    %% Suggested formats for scope id parsing:
    %%   "" -> "0"
    %%   "0" -> Scope id 0
    %%   "1" - "9", "10" - "99" -> "0"++S
    %%   "0"++DecimalScopeId -> decimal scope id
    %%   "25"++PercentEncoded -> Percent encoded interface name
    %%   S -> Interface name (Unicode?)
    %% Missing: translation from interface name into integer scope id.
    %% XXX: scope id is actually 32 bit, but we only have room for
    %% 16 bit in the second address word - ignore or fix (how)?
    ipv6_addr_scope(ScopeId, Ar, Br, N);
ipv6_addr_scope([C|Cs], Ar, Br, N, Sr) ->
    ipv6_addr_scope(Cs, Ar, Br, N, [C|Sr]).
%%
ipv6_addr_scope(ScopeId, [P], Br, N)
  when N =< 7, P =:= 16#fe80;
       N =< 7, P =:= 16#ff02 ->
    %% Optimized special case
    ipv6_addr_done([ScopeId,P], Br, N+1);
ipv6_addr_scope(ScopeId, Ar, Br, N) ->
    case lists:reverse(Br++dup(8-N, 0, Ar)) of
        [P,0|Xs] when P =:= 16#fe80; P =:= 16#ff02 ->
            list_to_tuple([P,ScopeId|Xs]);
        _ ->
            erlang:error(badarg)
    end.

ipv6_addr_done(Ar, Br, N, {D1,D2,D3,D4}) ->
    ipv6_addr_done(Ar, [((D3 bsl 8) bor D4),((D1 bsl 8) bor D2)|Br], N+2).

ipv6_addr_done(Ar, Br, N) ->
    ipv6_addr_done(Br++dup(8-N, 0, Ar)).

ipv6_addr_done(Ar) ->
    list_to_tuple(lists:reverse(Ar)).

%% Collect 1-4 Hex digits
hex(Cs) -> hex(Cs, [], 4).
%%
hex([C|Cs], R, N) when C >= $0, C =< $9, N > 0 ->
    hex(Cs, [C|R], N-1);
hex([C|Cs], R, N) when C >= $a, C =< $f, N > 0 ->
    hex(Cs, [C|R], N-1);
hex([C|Cs], R, N) when C >= $A, C =< $F, N > 0 ->
    hex(Cs, [C|R], N-1);
hex(Cs, [_|_]=R, _) when is_list(Cs) ->
    {lists:reverse(R),Cs};
hex(_, _, _) ->
    erlang:error(badarg).

%% Parse a reverse decimal integer string, empty is 0
dec16(Cs) -> dec16(Cs, 0).
%%
dec16([], I) -> I;
dec16([C|Cs], I) when C >= $0, C =< $9 ->
    case 10*I + (C - $0) of
        J when 16#ffff < J ->
            erlang:error(badarg);
        J ->
            dec16(Cs, J)
    end;
dec16(_, _) -> erlang:error(badarg).

%% Hex string to integer
hex_to_int(Cs) -> erlang:list_to_integer(Cs, 16).

%% Dup onto head of existing list
dup(0, _, L) ->
    L;
dup(N, E, L) when is_integer(N), N >= 1 ->
    dup(N-1, E, [E|L]).



%% Convert IPv4 address to ascii
%% Convert IPv6 / IPV4 address to ascii (plain format)
ntoa({A,B,C,D}) when ?ip(A,B,C,D) ->
    integer_to_list(A) ++ "." ++ integer_to_list(B) ++ "." ++ 
	integer_to_list(C) ++ "." ++ integer_to_list(D);
%% ANY
ntoa({0,0,0,0,0,0,0,0}) -> "::";
%% LOOPBACK
ntoa({0,0,0,0,0,0,0,1}) -> "::1";
%% IPV4 ipv6 host address
ntoa({0,0,0,0,0,0,A,B}) when ?ip6(0,0,0,0,0,0,A,B) ->
    "::" ++ dig_to_dec(A) ++ "." ++ dig_to_dec(B);
%% IPV4 non ipv6 host address
ntoa({0,0,0,0,0,16#ffff=X,A,B}) when ?ip6(0,0,0,0,0,X,A,B) ->
    "::ffff:" ++ dig_to_dec(A) ++ "." ++ dig_to_dec(B);
ntoa({A,B,C,D,E,F,G,H}) when ?ip6(A,B,C,D,E,F,G,H) ->
    if
        A =:= 16#fe80, B =/= 0;
        A =:= 16#ff02, B =/= 0 ->
            %% Find longest sequence of zeros, at least 2,
            %% to replace with "::"
            ntoa([A,0,C,D,E,F,G,H], []) ++ "%0" ++ integer_to_list(B);
        true ->
            %% Find longest sequence of zeros, at least 2,
            %% to replace with "::"
            ntoa([A,B,C,D,E,F,G,H], [])
    end;
ntoa(_) ->
    {error, einval}.

%% Find first double zero
ntoa([], R) ->
    ntoa_done(R);
ntoa([0,0|T], R) ->
    ntoa(T, R, 2);
ntoa([D|T], R) ->
    ntoa(T, [D|R]).

%% Count consecutive zeros
ntoa([], R, _) ->
    ntoa_done(R, []);
ntoa([0|T], R, N) ->
    ntoa(T, R, N+1);
ntoa([D|T], R, N) ->
    ntoa(T, R, N, [D]).

%% Find alternate double zero
ntoa([], R1, _N1, R2) ->
    ntoa_done(R1, R2);
ntoa([0,0|T], R1, N1, R2) ->
    ntoa(T, R1, N1, R2, 2);
ntoa([D|T], R1, N1, R2) ->
    ntoa(T, R1, N1, [D|R2]).

%% Count consecutive alternate zeros
ntoa(T, R1, N1, R2, N2) when N2 > N1 ->
    %% Alternate zero sequence is longer - use it instead
    ntoa(T, R2++dup(N1, 0, R1), N2);
ntoa([], R1, _N1, R2, N2) ->
    ntoa_done(R1, dup(N2, 0, R2));
ntoa([0|T], R1, N1, R2, N2) ->
    ntoa(T, R1, N1, R2, N2+1);
ntoa([D|T], R1, N1, R2, N2) ->
    ntoa(T, R1, N1, [D|dup(N2, 0, R2)]).

ntoa_done(R1, R2) ->
    lists:append(
      separate(":", lists:map(fun dig_to_hex/1, lists:reverse(R1)))++
      ["::"|separate(":", lists:map(fun dig_to_hex/1, lists:reverse(R2)))]).

ntoa_done(R) ->
    lists:append(separate(":", lists:map(fun dig_to_hex/1, lists:reverse(R)))).

separate(_E, []) ->
    [];
separate(E, [_|_]=L) ->
    separate(E, L, []).

separate(E, [H|[_|_]=T], R) ->
    separate(E, T, [E,H|R]);
separate(_E, [H], R) ->
    lists:reverse(R, [H]).

%% convert to A.B decimal form
dig_to_dec(0) -> "0.0";
dig_to_dec(X) -> 
    integer_to_list((X bsr 8) band 16#ff) ++ "." ++
	integer_to_list(X band 16#ff).

%% Convert a integer to hex string (lowercase)
dig_to_hex(0) -> "0";
dig_to_hex(X) when is_integer(X), 0 < X ->
    dig_to_hex(X, "").
%%
dig_to_hex(0, Acc) -> Acc;
dig_to_hex(X, Acc) ->
    dig_to_hex(
      X bsr 4,
      [case X band 15 of
           D when D < 10 -> D + $0;
           D -> D - 10 + $a
       end|Acc]).

%%
%% Count number of '.' in a name
%% return {Number of non-terminating dots, has-terminating dot?}
%%        {integer, bool}
%%
dots(Name) -> dots(Name, 0).

dots([$.], N) -> {N, true};
dots([$. | T], N) -> dots(T, N+1);
dots([_C | T], N) -> dots(T, N);
dots([], N) -> {N, false}.

