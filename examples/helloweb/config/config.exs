import Config

config :helloweb, ecto_repos: [Helloweb.Repo]
config :helloweb, Helloweb.Repo, pool_size: 5
