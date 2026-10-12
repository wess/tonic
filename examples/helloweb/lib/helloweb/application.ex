defmodule Helloweb.Application do
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      Helloweb.Repo,
      {Ecto.Migrator, repos: [Helloweb.Repo], migrator: &Helloweb.Migration.run/3},
      {Bandit, plug: Helloweb.Router, port: Application.fetch_env!(:helloweb, :port)}
    ]

    Supervisor.start_link(children, strategy: :one_for_one, name: Helloweb.Supervisor)
  end
end
