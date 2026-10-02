defmodule URI do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.























  @derive {Inspect, optional: [:authority]}
  defstruct [:scheme, :authority, :userinfo, :host, :port, :path, :query, :fragment]















  defmodule Error do






    defexception [:action, :reason, :part]


    def message(%Error{action: action, reason: reason, part: part}) do
      "cannot #{action} due to reason #{reason}: #{inspect(part)}"
    end
  end

  import Bitwise

  @reserved_characters ~c":/?#[]@!$&'()*+,;="
  @formatted_reserved_characters Enum.map_join(@reserved_characters, ", ", &<<?`, &1, ?`>>)


















  def default_port(scheme) when is_binary(scheme) do
    :elixir_config.get({:uri, scheme}, nil)
  end














  def default_port(scheme, port) when is_binary(scheme) and is_integer(port) and port >= 0 do
    :elixir_config.put({:uri, scheme}, port)
  end












































  def encode_query(enumerable, encoding \\ :www_form) do
    Enum.map_join(enumerable, "&", &encode_kv_pair(&1, encoding))
  end

  defp encode_kv_pair({key, _}, _encoding) when is_list(key) do
    raise ArgumentError, "encode_query/2 keys cannot be lists, got: #{inspect(key)}"
  end

  defp encode_kv_pair({_, value}, _encoding) when is_list(value) do
    raise ArgumentError, "encode_query/2 values cannot be lists, got: #{inspect(value)}"
  end

  defp encode_kv_pair({key, value}, :rfc3986) do
    encode(Kernel.to_string(key), &char_unreserved?/1) <>
      "=" <> encode(Kernel.to_string(value), &char_unreserved?/1)
  end

  defp encode_kv_pair({key, value}, :www_form) do
    encode_www_form(Kernel.to_string(key)) <> "=" <> encode_www_form(Kernel.to_string(value))
  end






































  def decode_query(query, map \\ %{}, encoding \\ :www_form)

  def decode_query(query, %_{} = dict, encoding) when is_binary(query) do
    IO.warn(
      "URI.decode_query/3 expects the second argument to be a map, other usage is deprecated"
    )

    decode_query_into_dict(query, dict, encoding)
  end

  def decode_query(query, map, encoding) when is_binary(query) and is_map(map) do
    decode_query_into_map(query, map, encoding)
  end

  def decode_query(query, dict, encoding) when is_binary(query) do
    IO.warn(
      "URI.decode_query/3 expects the second argument to be a map, other usage is deprecated"
    )

    decode_query_into_dict(query, dict, encoding)
  end

  defp decode_query_into_map(query, map, encoding) do
    case decode_next_query_pair(query, encoding) do
      nil ->
        map

      {{key, value}, rest} ->
        decode_query_into_map(rest, Map.put(map, key, value), encoding)
    end
  end

  defp decode_query_into_dict(query, dict, encoding) do
    case decode_next_query_pair(query, encoding) do
      nil ->
        dict

      {{key, value}, rest} ->
        # Avoid warnings about Dict being deprecated
        dict_module = String.to_atom("Dict")
        decode_query_into_dict(rest, dict_module.put(dict, key, value), encoding)
    end
  end
































  def query_decoder(query, encoding \\ :www_form) when is_binary(query) do
    Stream.unfold(query, &decode_next_query_pair(&1, encoding))
  end

  defp decode_next_query_pair("", _encoding) do
    nil
  end

  defp decode_next_query_pair(query, encoding) do
    {undecoded_next_pair, rest} =
      case :binary.split(query, "&") do
        [next_pair, rest] -> {next_pair, rest}
        [next_pair] -> {next_pair, ""}
      end

    next_pair =
      case :binary.split(undecoded_next_pair, "=") do
        [key, value] ->
          {decode_with_encoding(key, encoding), decode_with_encoding(value, encoding)}

        [key] ->
          {decode_with_encoding(key, encoding), ""}
      end

    {next_pair, rest}
  end

  defp decode_with_encoding(string, :www_form) do
    decode_www_form(string)
  end

  defp decode_with_encoding(string, :rfc3986) do
    decode(string)
  end














  def char_reserved?(character) do
    character in @reserved_characters
  end

















  def char_unreserved?(character) do
    character in ?0..?9 or character in ?a..?z or character in ?A..?Z or character in ~c"~_-."
  end















  def char_unescaped?(character) do
    char_reserved?(character) or char_unreserved?(character)
  end




































  def encode(string, predicate \\ &char_unescaped?/1)
      when is_binary(string) and is_function(predicate, 1) do
    for <<byte <- string>>, into: "", do: percent(byte, predicate)
  end















  def encode_www_form(string) when is_binary(string) do
    for <<byte <- string>>, into: "" do
      case percent(byte, &char_unreserved?/1) do
        "%20" -> "+"
        percent -> percent
      end
    end
  end

  defp percent(char, predicate) do
    if predicate.(char) do
      <<char>>
    else
      <<"%", hex(bsr(char, 4)), hex(band(char, 15))>>
    end
  end

  defp hex(n) when n <= 9, do: n + ?0
  defp hex(n), do: n + ?A - 10











  def decode(uri) do
    unpercent(uri, "", false)
  end















  def decode_www_form(string) when is_binary(string) do
    unpercent(string, "", true)
  end

  defp unpercent(<<?+, tail::binary>>, acc, spaces = true) do
    unpercent(tail, <<acc::binary, ?\s>>, spaces)
  end

  defp unpercent(<<?%, tail::binary>>, acc, spaces) do
    with <<hex1, hex2, tail::binary>> <- tail,
         dec1 when is_integer(dec1) <- hex_to_dec(hex1),
         dec2 when is_integer(dec2) <- hex_to_dec(hex2) do
      unpercent(tail, <<acc::binary, bsl(dec1, 4) + dec2>>, spaces)
    else
      _ -> unpercent(tail, <<acc::binary, ?%>>, spaces)
    end
  end

  defp unpercent(<<head, tail::binary>>, acc, spaces) do
    unpercent(tail, <<acc::binary, head>>, spaces)
  end

  defp unpercent(<<>>, acc, _spaces), do: acc


  defp hex_to_dec(n) when n in ?A..?F, do: n - ?A + 10
  defp hex_to_dec(n) when n in ?a..?f, do: n - ?a + 10
  defp hex_to_dec(n) when n in ?0..?9, do: n - ?0
  defp hex_to_dec(_n), do: nil








































































































  def new(%URI{} = uri), do: {:ok, uri}

  def new(binary) when is_binary(binary) do
    case :uri_string.parse(binary) do
      %{} = map -> {:ok, uri_from_map(map)}
      {:error, :invalid_uri, term} -> {:error, Kernel.to_string(term)}
    end
  end




































  def new!(%URI{} = uri), do: uri

  def new!(binary) when is_binary(binary) do
    case :uri_string.parse(binary) do
      %{} = map ->
        uri_from_map(map)

      {:error, reason, part} ->
        raise Error, action: :parse, reason: reason, part: Kernel.to_string(part)
    end
  end

  defp uri_from_map(%{path: ""} = map), do: uri_from_map(%{map | path: nil})

  defp uri_from_map(map) do
    uri = Map.merge(%URI{}, map)

    case map do
      %{scheme: scheme} ->
        scheme = String.downcase(scheme, :ascii)

        case map do
          %{port: port} when is_integer(port) ->
            %{uri | scheme: scheme}

          %{} ->
            %{uri | scheme: scheme, port: default_port(scheme)}
        end

      %{port: :undefined} ->
        %{uri | port: nil}

      %{} ->
        uri
    end
  end





































































































  def parse(%URI{} = uri), do: uri

  def parse(string) when is_binary(string) do
    # From https://tools.ietf.org/html/rfc3986#appendix-B
    # Parts:    12                        3  4          5       6  7        8 9
    regex = ~r{^(([a-z][a-z0-9\+\-\.]*):)?(//([^/?#]*))?([^?#]*)(\?([^#]*))?(#(.*))?}i

    parts = Regex.run(regex, string)

    destructure [
                  _full,
                  # 1
                  _scheme_with_colon,
                  # 2
                  scheme,
                  # 3
                  authority_with_slashes,
                  # 4
                  _authority,
                  # 5
                  path,
                  # 6
                  query_with_question_mark,
                  # 7
                  _query,
                  # 8
                  _fragment_with_hash,
                  # 9
                  fragment
                ],
                parts

    path = nilify(path)
    scheme = nilify(scheme)
    query = nilify_query(query_with_question_mark)
    {authority, userinfo, host, port} = split_authority(authority_with_slashes)

    scheme = scheme && String.downcase(scheme)
    port = port || (scheme && default_port(scheme))

    %URI{
      scheme: scheme,
      path: path,
      query: query,
      fragment: fragment,
      authority: authority,
      userinfo: userinfo,
      host: host,
      port: port
    }
  end

  defp nilify_query("?" <> query), do: query
  defp nilify_query(_other), do: nil

  # Split an authority into its userinfo, host and port parts.
  #
  # Note that the host field is returned *without* [] even if, according to
  # RFC3986 grammar, a native IPv6 address requires them.
  defp split_authority("") do
    {nil, nil, nil, nil}
  end

  defp split_authority("//") do
    {"", nil, "", nil}
  end

  defp split_authority("//" <> authority) do
    regex = ~r/(^(.*)@)?(\[[a-zA-Z0-9:.]*\]|[^:]*)(:(\d*))?/
    components = Regex.run(regex, authority)

    destructure [_, _, userinfo, host, _, port], components
    userinfo = nilify(userinfo)
    host = if nilify(host), do: host |> String.trim_leading("[") |> String.trim_trailing("]")
    port = if nilify(port), do: String.to_integer(port)

    {authority, userinfo, host, port}
  end

  # Regex.run returns empty strings sometimes. We want
  # to replace those with nil for consistency.
  defp nilify(""), do: nil
  defp nilify(other), do: other
















  defdelegate to_string(uri), to: String.Chars.URI

















  def merge(uri, rel)

  def merge(%URI{host: nil}, _rel) do
    raise ArgumentError, "you must merge onto an absolute URI"
  end

  def merge(_base, %URI{scheme: rel_scheme} = rel) when rel_scheme != nil do
    %{rel | path: remove_dot_segments_from_path(rel.path)}
  end

  def merge(%URI{} = base, %URI{host: host} = rel) when host != nil do
    %{rel | scheme: base.scheme, path: remove_dot_segments_from_path(rel.path)}
  end

  def merge(%URI{} = base, %URI{path: nil} = rel) do
    %{base | query: rel.query || base.query, fragment: rel.fragment}
  end

  def merge(%URI{} = base, %URI{} = rel) do
    new_path = merge_paths(base.path, rel.path)
    %{base | path: new_path, query: rel.query, fragment: rel.fragment}
  end

  def merge(base, rel) do
    merge(parse(base), parse(rel))
  end

  defp merge_paths(nil, rel_path), do: merge_paths("/", rel_path)
  defp merge_paths(_, "/" <> _ = rel_path), do: remove_dot_segments_from_path(rel_path)

  defp merge_paths(base_path, rel_path) do
    (path_to_segments(base_path) ++ [:+] ++ path_to_segments(rel_path))
    |> remove_dot_segments([])
    |> join_reversed_segments()
  end

  defp remove_dot_segments_from_path(nil), do: nil

  defp remove_dot_segments_from_path(path) do
    path_to_segments(path)
    |> remove_dot_segments([])
    |> join_reversed_segments()
  end

  defp path_to_segments(path) do
    case String.split(path, "/") do
      ["" | tail] -> [:/ | tail]
      segments -> segments
    end
  end

  defp remove_dot_segments([], acc), do: acc
  defp remove_dot_segments([:/ | tail], acc), do: remove_dot_segments(tail, [:/ | acc])
  defp remove_dot_segments(["."], acc), do: remove_dot_segments([], ["" | acc])
  defp remove_dot_segments(["." | tail], acc), do: remove_dot_segments(tail, acc)
  defp remove_dot_segments([".." | tail], [:/]), do: remove_dot_segments(tail, [:/])
  defp remove_dot_segments([".."], [_ | acc]), do: remove_dot_segments([], ["" | acc])
  defp remove_dot_segments([".." | tail], [_ | acc]), do: remove_dot_segments(tail, acc)
  defp remove_dot_segments([_, :+ | tail], acc), do: remove_dot_segments(tail, acc)
  defp remove_dot_segments([head | tail], acc), do: remove_dot_segments(tail, [head | acc])

  defp join_reversed_segments(segments) do
    case Enum.reverse(segments) do
      [:/ | tail] -> ["" | tail]
      list -> list
    end
    |> Enum.join("/")
  end



















  def append_query(%URI{} = uri, query) when is_binary(query) and uri.query in [nil, ""] do
    %{uri | query: query}
  end

  def append_query(%URI{} = uri, query) when is_binary(query) do
    if String.ends_with?(uri.query, "&") do
      %{uri | query: uri.query <> query}
    else
      %{uri | query: uri.query <> "&" <> query}
    end
  end



















  def append_path(%URI{}, "//" <> _ = path) do
    raise ArgumentError, ~s|path cannot start with "//", got: #{inspect(path)}|
  end

  def append_path(%URI{path: path} = uri, "/" <> rest = all) do
    cond do
      path == nil -> %{uri | path: all}
      path != "" and :binary.last(path) == ?/ -> %{uri | path: path <> rest}
      true -> %{uri | path: path <> all}
    end
  end

  def append_path(%URI{}, path) when is_binary(path) do
    raise ArgumentError, ~s|path must start with "/", got: #{inspect(path)}|
  end
end

defimpl String.Chars, for: URI do
  def to_string(%{host: host, path: path} = uri)
      when host != nil and is_binary(path) and
             path != "" and binary_part(path, 0, 1) != "/" do
    raise ArgumentError,
          ":path in URI must be empty or an absolute path if URL has a :host, got: #{inspect(uri)}"
  end

  def to_string(%{scheme: scheme, port: port, path: path, query: query, fragment: fragment} = uri) do
    uri =
      case scheme && URI.default_port(scheme) do
        ^port -> %{uri | port: nil}
        _ -> uri
      end

    # Based on https://tools.ietf.org/html/rfc3986#section-5.3
    authority = extract_authority(uri)

    IO.iodata_to_binary([
      if(scheme, do: [scheme, ?:], else: []),
      if(authority, do: ["//" | authority], else: []),
      if(path, do: path, else: []),
      if(query, do: ["?" | query], else: []),
      if(fragment, do: ["#" | fragment], else: [])
    ])
  end

  defp extract_authority(%{host: nil, authority: authority}) do
    authority
  end

  defp extract_authority(%{host: host, userinfo: userinfo, port: port}) do
    # According to the grammar at
    # https://tools.ietf.org/html/rfc3986#appendix-A, a "host" can have a colon
    # in it only if it's an IPv6 or "IPvFuture" address, so if there's a colon
    # in the host we can safely surround it with [].
    [
      if(userinfo, do: [userinfo | "@"], else: []),
      if(String.contains?(host, ":"), do: ["[", host | "]"], else: host),
      if(port, do: [":" | Integer.to_string(port)], else: [])
    ]
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/uri.ex (docs and specs stripped;
# line numbers match the original).
