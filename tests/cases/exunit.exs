ExUnit.start(seed: 0, max_cases: 1, autorun: false)

defmodule ExUnitCaseTest do
  use ExUnit.Case
  import ExUnit.CaptureIO
  import ExUnit.CaptureLog
  require Logger

  @moduletag :mod
  setup_all do
    {:ok, all: 1}
  end

  setup %{all: all} do
    {:ok, x: all + 1}
  end

  test "context", ctx do
    assert ctx.x == 2
    assert ctx.mod
    assert ctx.test == :"test context"
  end

  @tag skip: "not now"
  test "skipped" do
    flunk("no")
  end

  test "capture io" do
    assert capture_io(fn -> IO.puts("hi") end) == "hi\n"
    assert capture_io(:stderr, fn -> IO.puts(:stderr, "err") end) == "err\n"
  end

  test "capture log" do
    log = capture_log(fn -> Logger.error("boom") end)
    assert log =~ "[error] boom"
  end

  describe "group" do
    setup do: [y: 10]
    @describetag :d
    test "describe ctx", ctx do
      assert ctx.y == 10
      assert ctx.describe == "group"
      assert ctx.d
    end
  end

  test "failure" do
    assert [1, 2, 3] == [1, 2, 4]
  end

  test "raise" do
    raise ArgumentError, "bad"
  end
end

defmodule ParamTest do
  use ExUnit.Case, parameterize: [%{n: 1}, %{n: 2}]

  test "param", %{n: n} do
    assert n in [1, 2]
  end
end

%{failures: f, total: t, skipped: s} = ExUnit.run()
IO.inspect({f, t, s})
