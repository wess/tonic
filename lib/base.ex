defmodule Base do
  defp case_mode(opts), do: (case Keyword.get(opts, :case, :upper) do :upper -> 0; :lower -> 1; :mixed -> 2 end)

  def encode16(data, opts \\ []) when is_binary(data) do
    :tonic.base_encode(data, if(Keyword.get(opts, :case, :upper) == :lower, do: 1, else: 0), true, false)
  end

  def decode16(string, opts \\ []) do
    {:ok, decode16!(string, opts)}
  rescue
    ArgumentError -> :error
  end

  def decode16!(string, opts \\ [])

  def decode16!(string, opts) when is_binary(string) and rem(byte_size(string), 2) == 0 do
    result(:tonic.base_decode(string, 0, case_mode(opts), true))
  end

  def decode16!(string, _opts) when is_binary(string) do
    raise ArgumentError,
          "string given to decode has wrong length. An even number of bytes was expected, got: #{byte_size(string)}. " <>
            "Double check your string for unwanted characters or pad it accordingly"
  end

  def encode64(data, opts \\ []) when is_binary(data), do: :tonic.base_encode(data, 2, Keyword.get(opts, :padding, true), false)
  def url_encode64(data, opts \\ []) when is_binary(data), do: :tonic.base_encode(data, 3, Keyword.get(opts, :padding, true), false)

  def decode64(string, opts \\ []) when is_binary(string) do
    {:ok, decode64!(string, opts)}
  rescue
    ArgumentError -> :error
  end

  def decode64!(string, opts \\ []) when is_binary(string) do
    result(:tonic.base_decode(remove_ignored(string, opts[:ignore]), 2, 0, Keyword.get(opts, :padding, true)))
  end

  def url_decode64(string, opts \\ []) when is_binary(string) do
    {:ok, url_decode64!(string, opts)}
  rescue
    ArgumentError -> :error
  end

  def url_decode64!(string, opts \\ []) when is_binary(string) do
    result(:tonic.base_decode(remove_ignored(string, opts[:ignore]), 3, 0, Keyword.get(opts, :padding, true)))
  end

  def encode32(data, opts \\ []) when is_binary(data) do
    :tonic.base_encode(data, 4, Keyword.get(opts, :padding, true), Keyword.get(opts, :case, :upper) == :lower)
  end

  def hex_encode32(data, opts \\ []) when is_binary(data) do
    :tonic.base_encode(data, 5, Keyword.get(opts, :padding, true), Keyword.get(opts, :case, :upper) == :lower)
  end

  def decode32(string, opts \\ []) do
    {:ok, decode32!(string, opts)}
  rescue
    ArgumentError -> :error
  end

  def decode32!(string, opts \\ []) when is_binary(string) do
    result(:tonic.base_decode(string, 4, case_mode(opts), Keyword.get(opts, :padding, true)))
  end

  def hex_decode32(string, opts \\ []) do
    {:ok, hex_decode32!(string, opts)}
  rescue
    ArgumentError -> :error
  end

  def hex_decode32!(string, opts \\ []) when is_binary(string) do
    result(:tonic.base_decode(string, 5, case_mode(opts), Keyword.get(opts, :padding, true)))
  end

  def valid16?(string, opts \\ []), do: match?({:ok, _}, decode16(string, opts))
  def valid64?(string, opts \\ []), do: match?({:ok, _}, decode64(string, opts))
  def url_valid64?(string, opts \\ []), do: match?({:ok, _}, url_decode64(string, opts))
  def valid32?(string, opts \\ []), do: match?({:ok, _}, decode32(string, opts))
  def hex_valid32?(string, opts \\ []), do: match?({:ok, _}, hex_decode32(string, opts))

  defp remove_ignored(string, nil), do: string
  defp remove_ignored(string, :whitespace), do: for(<<c <- string>>, c not in ~c"\s\t\r\n", into: "", do: <<c>>)

  defp result({:ok, bin}), do: bin
  defp result(:padding), do: raise(ArgumentError, "incorrect padding")

  defp result({:bad_char, byte}) do
    raise ArgumentError, "non-alphabet character found: #{inspect(<<byte>>, binaries: :as_strings)} (byte #{byte})"
  end
end
