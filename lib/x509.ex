defmodule Tonic.X509 do
  @moduledoc false
  # X.509 certificates as :public_key decodes them, :plain or :otp, walked by
  # hand against the certificate schema. Extension values stay DER in the
  # :otp form, where OTP decodes each known extension.

  alias Tonic.Der

  def decode(der, form) when form in [:plain, :otp] do
    [{0x30, tbs, _}, {0x30, alg, _}, {3, signature, _}] = Der.items(Der.body(der))

    case form do
      :plain -> {:Certificate, tbs(tbs, :plain), algorithm(alg, :AlgorithmIdentifier), Der.bit_string(signature)}
      :otp -> {:OTPCertificate, tbs(tbs, :otp), algorithm(alg, :SignatureAlgorithm), Der.bit_string(signature)}
    end
  end

  defp tbs(der, form) do
    {version, fields} =
      case Der.items(der) do
        [{0xA0, explicit, _} | rest] -> {version(explicit), rest}
        rest -> {:v1, rest}
      end

    [{2, serial, _}, {0x30, sig, _}, {0x30, issuer, _}, {0x30, validity, _}, {0x30, subject, _}, {0x30, spki, _} | optional] =
      fields

    unique = fn tag -> Enum.find_value(optional, :asn1_NOVALUE, fn {t, v, _} -> if t == tag, do: v end) end

    extensions =
      case Enum.find(optional, &match?({0xA3, _, _}, &1)) do
        {_, value, _} ->
          [{0x30, list, _}] = Der.items(value)
          Enum.map(Der.items(list), &extension(elem(&1, 1)))

        nil -> :asn1_NOVALUE
      end

    case form do
      :plain ->
        {:TBSCertificate, version, Der.integer(serial), algorithm(sig, :AlgorithmIdentifier), name(issuer, :plain),
         validity(validity), name(subject, :plain), plain_key(spki), unique.(0x81), unique.(0x82), extensions}

      :otp ->
        {:OTPTBSCertificate, version, Der.integer(serial), algorithm(sig, :SignatureAlgorithm), name(issuer, :otp),
         validity(validity), name(subject, :otp), otp_key(spki), unique.(0x81), unique.(0x82), extensions}
    end
  end

  defp version(explicit) do
    [{2, v, _}] = Der.items(explicit)
    Enum.at([:v1, :v2, :v3], Der.integer(v))
  end

  def algorithm(der, tag) do
    case Der.items(der) do
      [{6, oid, _}] -> {tag, Der.oid(oid), :asn1_NOVALUE}
      [{6, oid, _}, {5, "", _}] when tag == :SignatureAlgorithm -> {tag, Der.oid(oid), :NULL}
      [{6, oid, _}, {_, _, raw}] -> {tag, Der.oid(oid), raw}
    end
  end

  defp name(der, form) do
    rdns =
      for {0x31, set, _} <- Der.items(der) do
        for {0x30, pair, _} <- Der.items(set) do
          [{6, oid, _}, {tag, value, raw}] = Der.items(pair)
          oid = Der.oid(oid)
          {:AttributeTypeAndValue, oid, if(form == :plain, do: raw, else: attribute(oid, tag, value))}
        end
      end

    {:rdnSequence, rdns}
  end

  defp attribute(oid, _tag, value) when oid in [{2, 5, 4, 6}, {2, 5, 4, 5}], do: String.to_charlist(value)
  defp attribute(_oid, 0x0C, value), do: {:utf8String, value}
  defp attribute(_oid, 0x13, value), do: {:printableString, String.to_charlist(value)}
  defp attribute(_oid, 0x14, value), do: {:teletexString, String.to_charlist(value)}
  defp attribute(_oid, 0x16, value), do: String.to_charlist(value)
  defp attribute(_oid, 0x1C, value), do: {:universalString, String.to_charlist(value)}
  defp attribute(_oid, 0x1E, value), do: {:bmpString, :unicode.characters_to_list(value, {:utf16, :big})}
  defp attribute(_oid, _tag, value), do: value

  defp validity(der) do
    [from, to] = Der.items(der)
    {:Validity, time(from), time(to)}
  end

  defp time({0x17, value, _}), do: {:utcTime, String.to_charlist(value)}
  defp time({0x18, value, _}), do: {:generalTime, String.to_charlist(value)}

  defp plain_key(der) do
    [{0x30, alg, _}, {3, key, _}] = Der.items(der)
    {:SubjectPublicKeyInfo, algorithm(alg, :AlgorithmIdentifier), Der.bit_string(key)}
  end

  defp otp_key(der) do
    [{0x30, alg, _}, {3, key, _}] = Der.items(der)
    key = Der.bit_string(key)

    case Der.items(alg) do
      [{6, oid, _} | params] ->
        case Der.oid(oid) do
          {1, 2, 840, 113549, 1, 1, 1} = rsa ->
            {:OTPSubjectPublicKeyInfo, {:PublicKeyAlgorithm, rsa, :NULL}, :public_key.der_decode(:RSAPublicKey, key)}

          {1, 2, 840, 10045, 2, 1} = ec ->
            {:OTPSubjectPublicKeyInfo, {:PublicKeyAlgorithm, ec, named_curve(params)}, {:ECPoint, key}}

          other ->
            raw = Enum.map(params, &elem(&1, 2))
            {:OTPSubjectPublicKeyInfo, {:PublicKeyAlgorithm, other, List.first(raw, :asn1_NOVALUE)}, key}
        end
    end
  end

  def named_curve([{6, oid, _} | _]), do: {:namedCurve, Der.oid(oid)}
  def named_curve(_), do: :asn1_NOVALUE

  defp extension(der) do
    case Der.items(der) do
      [{6, oid, _}, {1, <<flag>>, _}, {4, value, _}] -> {:Extension, Der.oid(oid), flag != 0, value}
      [{6, oid, _}, {4, value, _}] -> {:Extension, Der.oid(oid), false, value}
    end
  end
end
