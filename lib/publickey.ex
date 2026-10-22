defmodule :public_key do
  # The parts of public_key TLS clients use: PEM, certificate and key decoding
  # (Tonic.X509, Tonic.Der), the system trust store and the https host name
  # match fun.

  alias Tonic.{Der, X509}

  @cacerts :"$tonic_cacerts"

  ## PEM

  @pem_types %{
    "CERTIFICATE" => :Certificate,
    "PRIVATE KEY" => :PrivateKeyInfo,
    "ENCRYPTED PRIVATE KEY" => :EncryptedPrivateKeyInfo,
    "RSA PRIVATE KEY" => :RSAPrivateKey,
    "EC PRIVATE KEY" => :ECPrivateKey,
    "DSA PRIVATE KEY" => :DSAPrivateKey,
    "PUBLIC KEY" => :SubjectPublicKeyInfo,
    "RSA PUBLIC KEY" => :RSAPublicKey,
    "CERTIFICATE REQUEST" => :CertificationRequest,
    "X509 CRL" => :CertificateList,
    "EC PARAMETERS" => :EcpkParameters,
    "DH PARAMETERS" => :DHParameter
  }

  def pem_decode(pem) when is_binary(pem) do
    pem
    |> String.split(["\r\n", "\n"])
    |> pem_entries([])
  end

  defp pem_entries([], acc), do: Enum.reverse(acc)

  defp pem_entries([line | rest], acc) do
    case line do
      "-----BEGIN " <> label ->
        label = String.trim_trailing(label, "-")
        {body, rest} = Enum.split_while(rest, &(not String.starts_with?(&1, "-----END ")))
        rest = Enum.drop(rest, 1)
        encrypted = Enum.any?(body, &String.starts_with?(&1, "DEK-Info:"))
        data = body |> Enum.reject(&String.contains?(&1, ":")) |> Enum.join() |> String.trim()

        case {Map.fetch(@pem_types, label), Base.decode64(data, ignore: :whitespace)} do
          {{:ok, type}, {:ok, der}} -> pem_entries(rest, [{type, der, if(encrypted, do: :encrypted, else: :not_encrypted)} | acc])
          _ -> pem_entries(rest, acc)
        end

      _ ->
        pem_entries(rest, acc)
    end
  end

  def pem_encode(entries) do
    labels = Map.new(@pem_types, fn {label, type} -> {type, label} end)

    IO.iodata_to_binary(
      for {type, der, _} <- entries do
        label = Map.fetch!(labels, type)
        lines = der |> Base.encode64() |> chunks(64)
        ["-----BEGIN ", label, "-----\n", Enum.map(lines, &[&1, ?\n]), "-----END ", label, "-----\n\n"]
      end
    )
  end

  defp chunks(<<line::binary-size(64), rest::binary>>, n), do: [line | chunks(rest, n)]
  defp chunks("", _), do: []
  defp chunks(rest, _), do: [rest]

  def pem_entry_decode({:Certificate, der, :not_encrypted}), do: pkix_decode_cert(der, :plain)
  def pem_entry_decode({type, der, :not_encrypted}), do: der_decode(type, der)

  def pem_entry_encode(type, entity), do: {type, der_encode(type, entity), :not_encrypted}

  ## DER

  def der_decode(:Certificate, der), do: X509.decode(der, :plain)
  def der_decode(:OTPCertificate, der), do: X509.decode(der, :otp)

  def der_decode(:PrivateKeyInfo, der) do
    [{2, version, _}, {0x30, alg, _}, {4, key, _} | _] = Der.items(Der.body(der))
    [{6, oid, _} | params] = Der.items(alg)

    case Der.oid(oid) do
      {1, 2, 840, 10045, 2, 1} ->
        {:ECPrivateKey, ec_version, private, curve, public, attributes} = der_decode(:ECPrivateKey, key)
        curve = if curve == :asn1_NOVALUE, do: X509.named_curve(params), else: curve
        {:ECPrivateKey, ec_version, private, curve, public, attributes}

      {1, 2, 840, 113549, 1, 1, 1} ->
        der_decode(:RSAPrivateKey, key)

      other ->
        {:PrivateKeyInfo, Der.integer(version), {:PrivateKeyInfo_privateKeyAlgorithm, other, :asn1_NOVALUE}, key, :asn1_NOVALUE}
    end
  end

  def der_decode(:ECPrivateKey, der) do
    [{2, version, _}, {4, private, _} | rest] = Der.items(Der.body(der))
    curve = for {0xA0, value, _} <- rest, do: X509.named_curve(Der.items(value))
    public = for {0xA1, value, _} <- rest, {3, bits, _} <- Der.items(value), do: Der.bit_string(bits)

    {:ECPrivateKey, Der.integer(version), private, List.first(curve, :asn1_NOVALUE), List.first(public, :asn1_NOVALUE),
     :asn1_NOVALUE}
  end

  def der_decode(:RSAPrivateKey, der) do
    numbers = for {2, value, _} <- Der.items(Der.body(der)), do: Der.integer(value)
    [version | rest] = numbers
    List.to_tuple([:RSAPrivateKey, if(version == 0, do: :"two-prime", else: :multi) | Enum.take(rest, 8)] ++ [:asn1_NOVALUE])
  end

  def der_decode(:RSAPublicKey, der) do
    [{2, n, _}, {2, e, _}] = Der.items(Der.body(der))
    {:RSAPublicKey, Der.integer(n), Der.integer(e)}
  end

  def der_decode(:SubjectPublicKeyInfo, der) do
    [{0x30, alg, _}, {3, key, _}] = Der.items(Der.body(der))
    {:SubjectPublicKeyInfo, X509.algorithm(alg, :AlgorithmIdentifier), Der.bit_string(key)}
  end

  def der_decode(type, _der), do: :erlang.error({:badarg, type})

  # decoding only: nothing here needs to produce DER
  def der_encode(type, value), do: :erlang.error({:badarg, {:der_encode, type, value}})

  def pkix_decode_cert(der, form), do: X509.decode(der, form)

  ## Trust store

  def cacerts_get do
    case :persistent_term.get(@cacerts, nil) do
      nil ->
        case cacerts_load() do
          :ok -> :persistent_term.get(@cacerts)
          {:error, reason} -> :erlang.error({:failed_load_cacerts, reason})
        end

      certs ->
        certs
    end
  end

  def cacerts_load do
    case :tonic.ssl_cacerts() do
      ders when is_list(ders) -> store(ders)
      {:error, _} = error -> error
    end
  end

  def cacerts_load(file) do
    with {:ok, pem} <- File.read(file) do
      case for({:Certificate, der, :not_encrypted} <- pem_decode(pem), do: der) do
        [] -> {:error, :no_cacerts_found}
        ders -> store(ders)
      end
    end
  end

  def cacerts_clear, do: :persistent_term.erase(@cacerts)

  defp store(ders) do
    certs = for der <- ders, otp = safe_decode(der), do: {:cert, der, otp}
    :persistent_term.put(@cacerts, certs)
    :ok
  end

  defp safe_decode(der) do
    pkix_decode_cert(der, :otp)
  rescue
    _ -> nil
  end

  ## Host names

  def pkix_verify_hostname(cert, references), do: pkix_verify_hostname(cert, references, [])

  def pkix_verify_hostname(der, references, options) when is_binary(der),
    do: pkix_verify_hostname(pkix_decode_cert(der, :otp), references, options)

  def pkix_verify_hostname({:OTPCertificate, tbs, _, _} = cert, references, options) do
    extensions = elem(tbs, 10)
    names =
      if is_list(extensions) do
        Enum.flat_map(extensions, fn
          {:Extension, {2, 5, 29, 17}, _, value} -> hostname_names(value)
          _ -> []
        end)
      else
        []
      end

    names =
      if names == [] do
        {:rdnSequence, rdns} = elem(tbs, 6)
        for rdn <- rdns, {:AttributeTypeAndValue, {2, 5, 4, 3}, name} <- rdn,
          do: {:cn, hostname_text(name)}
      else
        names
      end

    references = Enum.map(references, fn {type, value} -> {type, hostname_text(value)} end)
    names = Enum.map(names, fn {type, value} -> {type, if(is_binary(value), do: :binary.bin_to_list(value), else: value)} end)
    fqdn_fun = Keyword.get(options, :fqdn_fun, fn _ -> :default end)
    fqdns = fn ->
      Enum.flat_map(references, fn reference ->
        value = try do
          case fqdn_fun.(reference) do
            :default -> hostname_fqdn(reference)
            value -> value
          end
        rescue
          _ -> :undefined
        catch
          _, _ -> :undefined
        end
        if is_list(value) and value != [] and :inet.parse_address(value) == {:error, :einval}, do: [value], else: []
      end)
    end
    if Enum.any?(names, &match?({:cn, _}, &1)) do
      hostname_loop(fqdns.(), names, options, cert)
    else
      hostname_loop(references, names, options, cert) or
        hostname_loop(Enum.map(fqdns.(), &{:dns_id, &1}), names, options, cert)
    end
  end

  defp hostname_fqdn({:dns_id, name}), do: name
  defp hostname_fqdn({:uri_id, uri}) do
    case :uri_string.normalize(uri, [:return_map]) do
      %{scheme: ~c"https", host: host} -> host
      _ -> :undefined
    end
  end
  defp hostname_fqdn(_), do: :undefined

  defp hostname_loop(references, names, options, cert) do
    match_fun = Keyword.get(options, :match_fun, fn _, _ -> :default end)
    fail = Keyword.get(options, :fail_callback, fn _ -> false end)
    Enum.any?(references, fn reference ->
      Enum.any?(names, fn presented ->
        result = case match_fun.(reference, presented) do
          :default -> hostname_match(reference, presented)
          result -> result == true
        end
        result or fail.(cert)
      end)
    end)
  end

  @doc false
  def hostname_names(value) when is_binary(value) do
    Enum.flat_map(Der.items(Der.body(value)), fn
      {0x82, name, _} -> [{:dNSName, String.to_charlist(name)}]
      {0x87, address, _} -> [{:iPAddress, address}]
      {0x86, uri, _} -> [{:uniformResourceIdentifier, String.to_charlist(uri)}]
      _ -> []
    end)
  end

  def hostname_names(names) when is_list(names), do: names

  defp hostname_text({_type, text}), do: hostname_text(text)
  defp hostname_text(text) when is_binary(text), do: String.to_charlist(text)
  defp hostname_text(text), do: text

  defp hostname_match({:dns_id, reference}, {:dNSName, name}), do: hostname_equal(reference, name)
  defp hostname_match({:uri_id, reference}, {:uniformResourceIdentifier, name}), do: hostname_equal(reference, name)
  defp hostname_match({:srv_id, reference}, {type, name}) when type in [:srvName, {1, 3, 6, 1, 4, 1, 434, 2, 2, 1, 37, 0}],
    do: hostname_equal(reference, name)
  defp hostname_match(reference, {:cn, name}) when is_list(reference), do: wildcard_match(reference, name)
  defp hostname_equal(a, b), do: String.downcase(to_string(a)) == String.downcase(to_string(b))

  defp hostname_match({:ip, reference}, {:iPAddress, address}) do
    ip =
      cond do
        is_tuple(reference) -> reference
        is_binary(reference) -> :inet.parse_address(String.to_charlist(reference)) |> hostname_ip()
        is_list(reference) -> :inet.parse_address(reference) |> hostname_ip()
        true -> nil
      end

    address = if is_list(address), do: :binary.list_to_bin(address), else: address
    case {ip, address} do
      {{a, b, c, d}, <<a, b, c, d>>} -> true
      {{a, b, c, d, e, f, g, h}, <<a::16, b::16, c::16, d::16, e::16, f::16, g::16, h::16>>} -> true
      _ -> false
    end
  end

  defp hostname_match(_, _), do: false
  defp hostname_ip({:ok, ip}), do: ip
  defp hostname_ip(_), do: nil

  def pkix_verify_hostname_match_fun(:https) do
    fn
      {:dns_id, [_ | _] = fqdn}, {:dNSName, [_ | _] = name} -> wildcard_match(fqdn, name)
      _, _ -> :default
    end
  end

  defp wildcard_match(fqdn, name) do
    [host | domain] = fqdn |> to_string() |> String.downcase() |> String.split(".")
    [first | rest] = name |> to_string() |> String.downcase() |> String.split(".")
    not String.contains?(host, "*") and wild?(host, first) and rest == domain
  end

  defp wild?(host, pattern) do
    case String.split(pattern, "*", parts: 2) do
      [^host] -> true
      [prefix, suffix] -> not String.contains?(suffix, "*") and byte_size(host) >= byte_size(prefix) + byte_size(suffix) and String.starts_with?(host, prefix) and String.ends_with?(host, suffix)
      _ -> false
    end
  end
end
