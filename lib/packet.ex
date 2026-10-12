defmodule Tonic.Packet do
  @moduledoc false
  # :erlang.decode_packet/3 for raw, length-prefixed, line and HTTP packets.

  @methods ~w(OPTIONS GET HEAD POST PUT DELETE TRACE)

  def decode(type, bin, opts) when is_binary(bin) and is_list(opts) do
    limit = Keyword.get(opts, :packet_size, 0)

    case type do
      t when t in [:raw, 0] -> if bin == "", do: {:more, :undefined}, else: {:ok, bin, ""}
      n when n in [1, 2, 4] -> prefixed(bin, n * 8, limit)
      :line -> line(bin, Keyword.get(opts, :line_length, 0), limit)
      t when t in [:http, :http_bin] -> http(bin, limit, t == :http)
      t when t in [:httph, :httph_bin] -> header(bin, limit, t == :httph)
      _ -> :erlang.error(:badarg, [type, bin, opts])
    end
  end

  defp prefixed(bin, bits, limit) do
    case bin do
      <<size::size(bits), _::binary>> when limit > 0 and size > limit -> {:error, :invalid}
      <<size::size(bits), body::binary-size(size), rest::binary>> -> {:ok, body, rest}
      <<size::size(bits), _::binary>> -> {:more, div(bits, 8) + size}
      _ -> {:more, :undefined}
    end
  end

  defp line(bin, max, limit) do
    case :binary.match(bin, "\n") do
      {at, 1} when limit > 0 and at + 1 > limit -> {:error, :invalid}
      {at, 1} -> {:ok, binary_part(bin, 0, at + 1), binary_part(bin, at + 1, byte_size(bin) - at - 1)}
      :nomatch when max > 0 and byte_size(bin) >= max -> {:ok, binary_part(bin, 0, max), binary_part(bin, max, byte_size(bin) - max)}
      :nomatch when limit > 0 and byte_size(bin) > limit -> {:error, :invalid}
      :nomatch -> {:more, :undefined}
    end
  end

  # One line ending in LF (CR before it is optional), or :more / error.
  defp next_line(bin, limit) do
    case :binary.match(bin, "\n") do
      {at, 1} when limit > 0 and at + 1 > limit ->
        {:error, :invalid}

      {at, 1} ->
        raw = binary_part(bin, 0, at + 1)
        rest = binary_part(bin, at + 1, byte_size(bin) - at - 1)
        text = raw |> String.trim_trailing("\n") |> String.trim_trailing("\r")
        {:ok, raw, text, rest}

      :nomatch when limit > 0 and byte_size(bin) > limit ->
        {:error, :invalid}

      :nomatch ->
        {:more, :undefined}
    end
  end

  defp http(bin, limit, charlists?) do
    with {:ok, raw, text, rest} <- next_line(bin, limit) do
      packet =
        case String.split(text, " ", parts: 3) do
          ["HTTP/" <> version, status, reason] -> response(version, status, reason, raw, charlists?)
          ["HTTP/" <> version, status] -> response(version, status, "", raw, charlists?)
          [method, uri, "HTTP/" <> version] when method != "" -> request(method, uri, version, raw, charlists?)
          _ -> {:http_error, out(raw, charlists?)}
        end

      {:ok, packet, rest}
    end
  end

  defp request(method, uri, version, raw, charlists?) do
    case version(version) do
      {:ok, version} ->
        method = if method in @methods, do: String.to_atom(method), else: out(method, charlists?)
        {:http_request, method, uri(uri, charlists?), version}

      :error ->
        {:http_error, out(raw, charlists?)}
    end
  end

  defp response(version, status, reason, raw, charlists?) do
    with {:ok, version} <- version(version),
         {code, ""} <- Integer.parse(status) do
      {:http_response, version, code, out(reason, charlists?)}
    else
      _ -> {:http_error, out(raw, charlists?)}
    end
  end

  defp version(text) do
    with [major, minor] <- String.split(text, "."),
         {major, ""} <- Integer.parse(major),
         {minor, ""} <- Integer.parse(minor) do
      {:ok, {major, minor}}
    else
      _ -> :error
    end
  end

  defp uri("*", _), do: :*
  defp uri("/" <> _ = path, charlists?), do: {:abs_path, out(path, charlists?)}

  defp uri(uri, charlists?) do
    case String.split(uri, "://", parts: 2) do
      [scheme, rest] when scheme in ["http", "https", "HTTP", "HTTPS"] ->
        {authority, path} =
          case :binary.match(rest, "/") do
            {at, 1} -> {binary_part(rest, 0, at), binary_part(rest, at, byte_size(rest) - at)}
            :nomatch -> {rest, "/"}
          end

        {host, port} =
          case String.split(authority, ":", parts: 2) do
            [host, port] -> {host, String.to_integer(port)}
            [host] -> {host, :undefined}
          end

        {:absoluteURI, String.to_atom(String.downcase(scheme)), out(host, charlists?), port, out(path, charlists?)}

      _ ->
        case String.split(uri, ":", parts: 2) do
          [scheme, rest] -> {:scheme, out(scheme, charlists?), out(rest, charlists?)}
          _ -> out(uri, charlists?)
        end
    end
  end

  @headers ~w(Cache-Control Connection Date Pragma Transfer-Encoding Upgrade Via Accept Accept-Charset
               Accept-Encoding Accept-Language Authorization From Host If-Modified-Since If-Match If-None-Match
               If-Range If-Unmodified-Since Max-Forwards Proxy-Authorization Range Referer User-Agent Age Location
               Proxy-Authenticate Public Retry-After Server Vary Warning Www-Authenticate Allow Content-Base
               Content-Encoding Content-Language Content-Length Content-Location Content-Md5 Content-Range
               Content-Type Etag Expires Last-Modified Accept-Ranges Set-Cookie Set-Cookie2 X-Forwarded-For Cookie
               Keep-Alive Proxy-Connection)

  # A header needs the next line's first byte: it may continue the value.
  defp header(bin, limit, charlists?) do
    with {:ok, raw, text, rest} <- next_line(bin, limit),
         {:ok, raw, text, rest} <- fold(raw, text, rest, limit) do
      packet =
        if text == "" do
          :http_eoh
        else
          case String.split(text, ":", parts: 2) do
            [name, value] when name != "" ->
              if String.contains?(name, [" ", "\t"]) do
                {:http_error, out(raw, charlists?)}
              else
                field(name, String.trim_leading(value, " ") |> String.trim_leading("\t"), charlists?)
              end

            _ ->
              {:http_error, out(raw, charlists?)}
          end
        end

      {:ok, packet, rest}
    end
  end

  defp fold(raw, "", rest, _limit), do: {:ok, raw, "", rest}
  defp fold(_raw, _text, "", _limit), do: {:more, :undefined}

  defp fold(raw, text, <<c, _::binary>> = rest, limit) when c in [?\s, ?\t] do
    with {:ok, more_raw, more_text, rest} <- next_line(rest, limit) do
      fold(raw <> more_raw, text <> "\r\n" <> more_text, rest, limit)
    end
  end

  defp fold(raw, text, rest, _limit), do: {:ok, raw, text, rest}

  defp field(name, value, charlists?) do
    canonical = name |> String.split("-") |> Enum.map_join("-", &capitalize/1)

    case Enum.find_index(@headers, &(&1 == canonical)) do
      nil -> {:http_header, 0, out(canonical, charlists?), out(name, charlists?), out(value, charlists?)}
      index -> {:http_header, index + 1, String.to_atom(canonical), out(name, charlists?), out(value, charlists?)}
    end
  end

  defp capitalize(""), do: ""
  defp capitalize(<<c, rest::binary>>), do: String.upcase(<<c>>) <> String.downcase(rest)

  defp out(text, true), do: String.to_charlist(text)
  defp out(text, false), do: text
end
