init = function(args)
  local r = {}
  for i=1,16 do
    r[i] = wrk.format(nil, nil, nil, nil)
  end
  req = table.concat(r)
end

request = function()
  return req
end
