# :calendar RFC 3339 conversions: units, offsets, fractions and error terms.
# Every case passes an explicit offset so the output is timezone independent.
safe = fn f ->
  try do
    f.()
  rescue
    e -> {:raised, e.__struct__, Exception.message(e)}
  end
end

for t <- [0, -1, 86399, 951_782_400, 1_518_000_000, -62_167_219_200, -62_167_219_201, 253_402_300_799, 253_402_300_800] do
  IO.inspect({t, safe.(fn -> :calendar.system_time_to_rfc3339(t, offset: ~c"Z") end),
              safe.(fn -> :calendar.system_time_to_rfc3339(t, offset: ~c"-05:30") end),
              safe.(fn -> :calendar.system_time_to_rfc3339(t, offset: 3600) end)})
end

for t <- [999, -1, -1500, 1_518_000_000_123], u <- [:millisecond, :microsecond, :nanosecond] do
  IO.inspect({t, u, :calendar.system_time_to_rfc3339(t, unit: u, offset: ~c"z"),
              :calendar.system_time_to_rfc3339(t, unit: u, offset: -1500)})
end

IO.inspect(:calendar.system_time_to_rfc3339(1_700_000_000_123_456_789, unit: :native, offset: 1_000_000_000))
IO.inspect(:calendar.system_time_to_rfc3339(1_518_000_000, offset: ~c"+00:00", time_designator: ?\s))
IO.inspect(safe.(fn -> :calendar.system_time_to_rfc3339(1, offset: ~c"+1") end))
IO.inspect(safe.(fn -> :calendar.system_time_to_rfc3339(1, offset: ~c"*01:00") end))
IO.inspect(safe.(fn -> :calendar.system_time_to_rfc3339(1, unit: :minute, offset: ~c"Z") end))

strings = [
  "2018-02-01T16:17:58+01:00", "2018-02-01 15:18:02.088Z", "1969-12-31T23:59:59.5Z", "0000-01-01T00:00:00+00:01",
  "9999-12-31T23:59:59-00:01", "2023-02-29T12:00:00Z", "2024-13-01T00:00:00Z", "2024-01-00T00:00:00Z",
  "2024-01-01T24:00:00Z", "2024-01-01T00:00:00", "2024-01-01T00:00:00+05", "2024-01-01T00:00:00.123456789Z",
  "2024-01-01T00:00:00.9999999999Z", "2024-01-01T00:00:00.Z", "2024-01-01T00:00:00+0530", "abcd-01-01T00:00:00Z",
  "1960-06-15T08:30:00.25-03:00"
]

for s <- strings do
  IO.inspect({s, safe.(fn -> :calendar.rfc3339_to_system_time(String.to_charlist(s)) end),
              safe.(fn -> :calendar.rfc3339_to_system_time(String.to_charlist(s), unit: :millisecond) end),
              safe.(fn -> :calendar.rfc3339_to_system_time(String.to_charlist(s), unit: :nanosecond) end)})
end

for t <- [0, 1_518_000_000_123, -1_000] do
  s = :calendar.system_time_to_rfc3339(t, unit: :millisecond, offset: ~c"+03:00")
  IO.inspect({t, s, :calendar.rfc3339_to_system_time(s, unit: :millisecond)})
end

IO.inspect({safe.(fn -> :calendar.valid_date(-1, 2, 29) end), safe.(fn -> :calendar.valid_date(:a, 2, 29) end),
            safe.(fn -> :calendar.date_to_gregorian_days(2023, 2, 29) end), :calendar.seconds_to_daystime(-86401)})
