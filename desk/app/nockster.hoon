::  Browser wallet assets and an opaque, owner-only encrypted backup.
/+  default-agent
|%
+$  state-0  [%0 revision=@ud backup=(unit octs)]
+$  card  card:agent:gall
++  respond
  |=  [rid=@ta payload=simple-payload:http]
  ^-  (list card)
  =/  path  /http-response/[rid]
  :~  [%give %fact ~[path] %http-response-header !>(response-header.payload)]
      [%give %fact ~[path] %http-response-data !>(data.payload)]
      [%give %kick ~[path] ~]
  ==
--
=|  state-0
=*  state  -
^-  agent:gall
|_  =bowl:gall
+*  this  .
    def  ~(. (default-agent this %|) bowl)
++  on-init
  ^-  (quip card _this)
  :_  this
  :~  [%pass /bind %arvo %e %connect [~ /apps/nockster] dap.bowl]
      [%pass /bind %arvo %e %connect [~ /nockster/backup] dap.bowl]
      [%pass /bind %arvo %e %connect [~ /nockster/accounts] dap.bowl]
  ==
++  on-save
  ^-  vase
  !>(state)
++  on-load
  |=  saved=vase
  ^-  (quip card _this)
  on-init(state !<(state-0 saved))
++  on-poke
  |=  [=mark =vase]
  ^-  (quip card _this)
  |^
    ?>  =(our.bowl src.bowl)
    ?>  =(%handle-http-request mark)
    =/  [rid=@ta req=inbound-request:eyre]  !<([@ta inbound-request:eyre] vase)
    ?.  authenticated.req
      (reply rid 403 'text/plain' `(as-octs:mimes:html 'Sign in to your ship.'))
    ?:  =('/nockster/backup' url.request.req)
      (handle-backup rid req)
    ?:  =('/nockster/accounts/' (crip (scag 19 (trip url.request.req))))
      (proxy-accounts rid req)
    ?.  =('GET' method.request.req)
      (reply rid 405 'text/plain' ~)
    ?:  =('/apps/nockster' url.request.req)
      :_  this
      %-  respond
      :-  rid
      [[307 ['location' '/apps/nockster/'] ~] ~]
    (serve-file rid url.request.req)
  ++  proxy-accounts
    |=  [rid=@ta req=inbound-request:eyre]
    ^-  (quip card _this)
    ?.  =('POST' method.request.req)
      (reply rid 405 'text/plain' ~)
    ?.  =(`'1' (get-header:http 'x-nockster-proxy' header-list.request.req))
      (reply rid 403 'text/plain' ~)
    =/  target=(unit @t)
      ?:  =('/nockster/accounts/auth/iris/challenge' url.request.req)
        `'https://nockblocks.com/auth/iris/challenge'
      ?:  =('/nockster/accounts/auth/iris/token' url.request.req)
        `'https://nockblocks.com/auth/iris/token'
      ?:  =('/nockster/accounts/auth/keys' url.request.req)
        `'https://nockblocks.com/auth/keys'
      ~
    ?~  target
      (reply rid 404 'text/plain' ~)
    =/  body=(unit octs)  body.request.req
    ?~  body
      (reply rid 400 'text/plain' ~)
    ?:  (gth p.u.body 65.536)
      (reply rid 413 'text/plain' ~)
    =/  headers=header-list:http
      ~[['content-type' 'application/json'] ['accept' 'application/json']]
    =/  origin=(unit @t)  (get-header:http 'origin' header-list.request.req)
    =?  headers  ?=(^ origin)  [['origin' u.origin] headers]
    ::  Relay only the explicit accounts session, never the Eyre login cookie.
    =/  cookie=(unit @t)  (get-header:http 'x-nockster-session' header-list.request.req)
    =?  headers  &(?=(^ cookie) =('https://nockblocks.com/auth/keys' u.target))
      [['cookie' u.cookie] headers]
    =/  kind=@tas  ?:(=('https://nockblocks.com/auth/iris/token' u.target) %token %json)
    :_  this
    :~  [%pass /proxy/[rid]/[kind] %arvo %i %request [%'POST' u.target headers body] [0 0]]
    ==
  ++  handle-backup
    |=  [rid=@ta req=inbound-request:eyre]
    ^-  (quip card _this)
    ?:  =('GET' method.request.req)
      (reply rid 200 'application/octet-stream' backup)
    ?.  =('PUT' method.request.req)
      (reply rid 405 'text/plain' ~)
    ::  A non-simple header prevents cross-origin form writes; no CORS is granted.
    ?.  =(`'1' (get-header:http 'x-nockster-backup' header-list.request.req))
      (reply rid 403 'text/plain' ~)
    ?.  =(`etag (get-header:http 'if-match' header-list.request.req))
      (reply rid 409 'text/plain' ~)
    =/  body=(unit octs)  body.request.req
    ?~  body
      (reply rid 400 'text/plain' ~)
    ?.  ?&  (gth p.u.body 0)  (lte p.u.body 2.097.152)  ==
      (reply rid 413 'text/plain' ~)
    =.  backup  body
    =.  revision  +(revision)
    (reply rid 204 'application/octet-stream' ~)
  ++  etag
    ^-  @t
    (rap 3 ~['"' (crip (scow %ud revision)) '"'])
  ++  serve-file
    |=  [rid=@ta url=@t]
    ^-  (quip card _this)
    =/  parsed
      (rush url ;~(plug apat:de-purl:html yque:de-purl:html))
    ?~  parsed
      (reply rid 404 'text/plain' ~)
    =/  [[ext=(unit @ta) site=(list @t)] args=(list [@t @t])]  u.parsed
    ?.  =(/apps/nockster (scag 2 site))
      (reply rid 404 'text/plain' ~)
    =/  file=path
      ?~  ext
        /web/index/html
      (weld /web (snoc (slag 2 site) u.ext))
    =/  path  [(scot %p our.bowl) q.byk.bowl (scot %da now.bowl) file]
    ?.  .^(? %cu path)
      (reply rid 404 'text/plain' ~)
    =/  content=^vase  .^(^vase %cr path)
    =/  kind=@ta  (rear file)
    =/  convert=tube:clay
      .^(tube:clay %cc (scot %p our.bowl) q.byk.bowl (scot %da now.bowl) /[kind]/mime)
    =/  resource=mime  !<(mime (convert content))
    (reply rid 200 (rsh 3^1 (spat p.resource)) `q.resource)
  ++  reply
    |=  [rid=@ta status=@ud content-type=@t data=(unit octs)]
    ^-  (quip card _this)
    :_  this
    %-  respond
    :-  rid
    :_  data
    :-  status
    :~  ['content-type' content-type]
        ['cache-control' 'no-store']
        ['x-content-type-options' 'nosniff']
        ['etag' etag]
        ['referrer-policy' 'no-referrer']
    ==
  --
++  on-peek
  |=  =path
  ^-  (unit (unit cage))
  [~ ~]
++  on-watch
  |=  =path
  ^-  (quip card _this)
  ?>  =(our.bowl src.bowl)
  ?>  ?=([%http-response @ ~] path)
  [~ this]
++  on-arvo
  |=  [=wire sign=sign-arvo]
  ^-  (quip card _this)
  |^
    ?:  =(/bind wire)
      ?>  ?=(%bound +<.sign)
      ?>  accepted.sign
      [~ this]
    ?>  ?=([%proxy @ @ ~] wire)
    ?>  ?=([%iris %http-response *] sign)
    =/  rid=@ta  i.t.wire
    =/  result=client-response:iris  +>.sign
    ?:  ?=(%progress -.result)
      ?:  (lte bytes-read.result 1.048.576)
        [~ this]
      [[%pass wire %arvo %i %cancel-request ~]~ this]
    ?:  ?=(%cancel -.result)
      (proxy-reply rid 502 ~ ~)
    =/  data=(unit octs)  ?~(full-file.result ~ `data.u.full-file.result)
    =/  session=(unit @t)
      ?:  =(%token i.t.t.wire)
        (get-header:http 'set-cookie' headers.response-header.result)
      ~
    (proxy-reply rid status-code.response-header.result data session)
  ++  proxy-reply
    |=  [rid=@ta status=@ud data=(unit octs) session=(unit @t)]
    ^-  (quip card _this)
    =/  headers=header-list:http
      :~  ['content-type' 'application/json']
          ['cache-control' 'no-store']
          ['x-content-type-options' 'nosniff']
      ==
    =?  headers  ?=(^ session)  [['x-nockster-session' u.session] headers]
    :_  this
    (respond rid [[status headers] data])
  --
++  on-agent
  |=  [=wire =sign:agent:gall]
  (on-agent:def wire sign)
++  on-leave
  |=  =path
  (on-leave:def path)
++  on-fail
  |=  [=term =tang]
  (on-fail:def term tang)
--
