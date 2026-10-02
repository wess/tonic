defmodule PackageCoverage do
  def main(_) do
    encoded = Jason.encode!(%{"message" => "ok", "items" => [1, true, nil]})
    decoded = Jason.decode!(encoded)
    IO.inspect({decoded["message"], decoded["items"]})
    schema = [retries: [type: :non_neg_integer, default: 3], mode: [type: {:in, [:fast, :safe]}, required: true]]
    validated = NimbleOptions.validate!([mode: :safe], schema)
    invalid = case NimbleOptions.validate([mode: :unknown], schema) do
      {:error, _} -> true
      _ -> false
    end
    IO.inspect({validated[:retries], validated[:mode], invalid})
    merged = DeepMerge.deep_merge(%{config: %{retries: 1, enabled: true}, keep: "left"}, %{config: %{retries: 3, name: "merged"}})
    IO.inspect({merged.config.retries, merged.config.enabled, merged.config.name, merged.keep})
  end
end
