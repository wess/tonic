# URI and :uri_string against OTP's own parser: malformed hosts, ports and
# percent-encoding, IPv6 literals, Unicode and relative references.
uris = [
  "http://example.com", "https://u:p@example.com:8080/p/a?q=1&b=2#frag", "http://bad host",
  "http://h:port", "http://ü.com", "http://h/%zz", "http://h/%2", "http://[::1]:80/", "http://[::1/",
  "http://[fe80::1%25en0]/", "http://[v1.fe]/", "http://[::ffff:1.2.3.4]/", "http://[]/", "http://::1/",
  "http://1.2.3.4:65536/", "http://h:/", "http://h:00080", "http://h:1x/", "http://a%20b.com/",
  "foo:bar", "mailto:a@b.c", "file:///etc/hosts", "//host/path", "rel/path", "a:b:c", "./a:b",
  "1http://x", "x_y://h", "HTTP://EXAMPLE.COM/%7Efoo/../bar/./baz", "http://a/b/c/./../../g",
  "http://x/?q=a b", "http://x/#a#b", "http://ü/ä?ö#ß", "http://x/<>", "http://x/\\", "http://@host",
  "http://a@b@c/", "http://u:p:q@h/", "http://h:80:80/", "?q", "#f", "//", "///", "http:", "http://",
  " http://h", "http://h/\n", "HTTP://H.COM:80/", "http://h.com:443/", "http://h/%2e%2e/x",
  "http://h/é%C3%A9", "http://h:99999999999999999999", ""
]

safe = fn f ->
  try do
    f.()
  rescue
    e -> {:raised, e.__struct__}
  catch
    :throw, t -> {:thrown, t}
  end
end

# Atom-keyed map print order is a separate known difference; sort to keep it out.
sorted = fn
  %{} = m -> Enum.sort(Map.to_list(m))
  other -> other
end

for u <- uris do
  parsed = :uri_string.parse(u)

  IO.inspect(
    {u, URI.new(u), safe.(fn -> URI.parse(u) |> URI.to_string() end), sorted.(parsed),
     safe.(fn -> :uri_string.normalize(u) end),
     safe.(fn -> if is_map(parsed), do: :uri_string.recompose(parsed), else: parsed end),
     safe.(fn -> :uri_string.resolve(u, "http://a/b/c/d;p?q") end)},
    limit: :infinity
  )
end

for u <- ["http://ü.com", "a:b/c?d", "a b+c%20%zz"] do
  IO.inspect({URI.encode(u), URI.encode_www_form(u), safe.(fn -> URI.decode(u) end), URI.decode_www_form(u),
              :uri_string.quote(u), safe.(fn -> :uri_string.unquote(u) end), safe.(fn -> :uri_string.percent_decode(u) end)})
end

IO.inspect(sorted.(:uri_string.parse(~c"http://h:8080/p?q#f")))
IO.inspect(:uri_string.compose_query([{"a", "b c"}, {"d", true}, {"ä", "€"}]))
IO.inspect(:uri_string.compose_query([{"a", "b c"}], [{:encoding, :rfc1738}]))
IO.inspect(:uri_string.dissect_query("a=1&b=%zz&c"))
IO.inspect(:uri_string.recompose(%{scheme: "http", host: "::1", port: 8080, path: "/x", userinfo: "u"}))
IO.inspect(:uri_string.normalize(%{scheme: "HTTP", host: "H", port: 80, path: "/a/../b"}))
IO.inspect(sorted.(:uri_string.normalize("http://h/a b", [:return_map])))
IO.inspect(:uri_string.transcode(<<"http://h/%E4">>, [{:in_encoding, :latin1}, {:out_encoding, :utf8}]))
IO.inspect(URI.decode_query("a=b+c&d=%zz&e"))
IO.inspect(safe.(fn -> URI.new!("http://bad host") end))
IO.inspect({:inet.parse_address(~c"fe80::1%en0"), :inet.parse_strict_address(~c"1.2.3"), :inet.ntoa({0, 0, 0, 0, 0, 65535, 258, 772})})
