defmodule Helloweb.Router do
  use Plug.Router

  alias Helloweb.{Greeting, Repo}

  plug Plug.Logger
  plug :match
  plug Plug.Parsers, parsers: [:json], pass: ["application/json"], json_decoder: Jason
  plug :dispatch

  get "/" do
    send_resp(conn, 200, "Hello, web!\n")
  end

  get "/greetings" do
    json(conn, 200, Repo.all(Greeting))
  end

  get "/greetings/:id" do
    with {id, ""} <- Integer.parse(id),
         %Greeting{} = greeting <- Repo.get(Greeting, id) do
      json(conn, 200, greeting)
    else
      _ -> json(conn, 404, %{error: "not found"})
    end
  end

  post "/greetings" do
    case %Greeting{} |> Greeting.changeset(conn.body_params) |> Repo.insert() do
      {:ok, greeting} -> json(conn, 201, greeting)
      {:error, changeset} -> json(conn, 422, %{errors: errors(changeset)})
    end
  end

  match _ do
    json(conn, 404, %{error: "not found"})
  end

  defp json(conn, status, body) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(status, Jason.encode!(body))
  end

  defp errors(changeset) do
    Ecto.Changeset.traverse_errors(changeset, fn {message, _opts} -> message end)
  end
end
