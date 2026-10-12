defmodule Helloweb.Migration do
  use Ecto.Migration

  def change do
    create_if_not_exists table(:greetings) do
      add :name, :string, null: false
      timestamps()
    end
  end

  def run(repo, direction, opts) do
    Ecto.Migrator.run(repo, [{1, __MODULE__}], direction, opts)
  end
end
