defmodule Helloweb.Greeting do
  use Ecto.Schema
  import Ecto.Changeset

  @derive {Jason.Encoder, only: [:id, :name, :inserted_at]}
  schema "greetings" do
    field :name, :string
    timestamps()
  end

  def changeset(greeting, attrs) do
    greeting
    |> cast(attrs, [:name])
    |> validate_required([:name])
    |> validate_length(:name, max: 100)
  end
end
