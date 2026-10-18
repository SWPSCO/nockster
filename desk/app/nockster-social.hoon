::  Public Nockchain address discovery over Ames and Gall's scry namespace.
/-  nockster-social
/+  default-agent
|%
+$  card  card:agent:gall
+$  state-0  [%0 revision=@ud published=(unit @t)]
+$  request  [ship=@p until=@da revision=(unit @ud)]
++  bind-http
  |=  dap=term
  ^-  card
  [%pass /bind %arvo %e %connect [~ /nockster/social] dap]
++  valid-address
  |=  address=@t
  ^-  ?
  =/  chars  (trip address)
  ?&  (gte (lent chars) 30)
      (lte (lent chars) 128)
      %+  levy
        chars
      |=(c=@t (lien (trip '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz') |=(a=@t =(a c))))
  ==
++  target
  |=  [ship=@p revision=@ud]
  ^-  spar:ames
  [ship /g/x/(scot %ud revision)/nockster-social//1/address]
++  record
  |=  [ship=@p address=(unit @t)]
  ^-  json
  (pairs:enjs:format ~[['ship' s+(scot %p ship)] ['address' ?~(address ~ s+u.address)]])
++  respond
  |=  [rid=@ta status=@ud data=(unit json)]
  ^-  (list card)
  =/  path  /http-response/[rid]
  =/  body=(unit octs)
    ?~  data  ~
    `(as-octs:mimes:html (en:json:html u.data))
  =/  header=response-header:http
    [status ~[['content-type' 'application/json'] ['cache-control' 'no-store'] ['x-content-type-options' 'nosniff']]]
  :~  [%give %fact ~[path] %http-response-header !>(header)]
      [%give %fact ~[path] %http-response-data !>(body)]
      [%give %kick ~[path] ~]
  ==
++  finish-request
  |=  [pending=(map @ta request) rid=@ta status=@ud data=(unit json)]
  ^-  (quip card (map @ta request))
  =/  req  (~(get by pending) rid)
  ?~  req  [~ pending]
  :_  (~(del by pending) rid)
  ;:  welp
      (respond rid status data)
      ^-  (list card)
      :~  [%pass /lookup/[rid] %agent [ship.u.req %nockster-social] %leave ~]
          [%pass /timeout/[rid] %arvo %b %rest until.u.req]
      ==
      ^-  (list card)
      ?~  revision.u.req  ~
      ~[[%pass /scry/[rid] %arvo %a %yawn (target ship.u.req u.revision.u.req)]]
  ==
--
=|  state-0
=*  state  -
=|  pending=(map @ta request)
^-  agent:gall
|_  =bowl:gall
+*  this  .
    def  ~(. (default-agent this %|) bowl)
++  on-init
  ^-  (quip card _this)
  :_  this(revision 1)
  :~  (bind-http dap.bowl)
      [%pass /publish %grow /address [%nockster-address [our.bowl ~]]]
  ==
++  on-save
  !>(state)
++  on-load
  |=  saved=vase
  ^-  (quip card _this)
  [~[(bind-http dap.bowl)] this(state !<(state-0 saved), pending ~)]
++  on-poke
  |=  [=mark =vase]
  ^-  (quip card _this)
  |^
    ?>  =(our.bowl src.bowl)
    ?>  =(%handle-http-request mark)
    =/  [rid=@ta req=inbound-request:eyre]  !<([@ta inbound-request:eyre] vase)
    ?.  authenticated.req
      (reply rid 403 ~)
    ?:  =('/nockster/social' url.request.req)
      ?:  =('GET' method.request.req)
        (reply rid 200 `(record our.bowl published))
      (publish rid req)
    ?.  =('GET' method.request.req)
      (reply rid 405 ~)
    =/  parsed  (rush url.request.req apat:de-purl:html)
    ?~  parsed
      (reply rid 404 ~)
    ?.  ?=([~ [%nockster %social %lookup @ ~]] u.parsed)
      (reply rid 404 ~)
    =/  who=(unit @p)  (slaw %p i.t.t.t.q.u.parsed)
    ?~  who
      (reply rid 400 ~)
    ?.  =((scot %p u.who) i.t.t.t.q.u.parsed)
      (reply rid 400 ~)
    ?:  =(our.bowl u.who)
      (reply rid 200 `(record our.bowl published))
    ?:  (gte (lent ~(tap by pending)) 32)
      (reply rid 429 ~)
    =/  until=@da  (add now.bowl ~s20)
    =.  pending  (~(put by pending) rid [u.who until ~])
    :_  this
    :~  [%pass /lookup/[rid] %agent [u.who %nockster-social] %watch /head]
        [%pass /timeout/[rid] %arvo %b %wait until]
    ==
  ++  reply
    |=  [rid=@ta status=@ud data=(unit json)]
    ^-  (quip card _this)
    [(respond rid status data) this]
  ++  publish
    |=  [rid=@ta req=inbound-request:eyre]
    ^-  (quip card _this)
    ?.  ?|  =('PUT' method.request.req)  =('DELETE' method.request.req)  ==
      (reply rid 405 ~)
    ?.  =(`'1' (get-header:http 'x-nockster-social' header-list.request.req))
      (reply rid 403 ~)
    =/  address=(unit @t)  ~
    =.  address
      ?:  =('DELETE' method.request.req)  ~
      =/  body=(unit octs)  body.request.req
      ?~  body  ~
      ?:  (gth p.u.body 128)  ~
      `q.u.body
    ?:  &(!=('DELETE' method.request.req) ?=(~ address))
      (reply rid 400 ~)
    ?:  &(?=(^ address) !(valid-address u.address))
      (reply rid 400 ~)
    ?:  =(address published)
      (reply rid 200 `(record our.bowl published))
    =.  published  address
    =.  revision  +(revision)
    =^  cards  this  (reply rid 200 `(record our.bowl published))
    :_  this
    :_  cards
    [%pass /publish %grow /address [%nockster-address [our.bowl published]]]
  --
++  on-peek
  |=  =path
  ^-  (unit (unit cage))
  ?+  path  [~ ~]
    [%x %address ~]  ``nockster-address+!>([our.bowl published])
    [%x %head ~]  ``noun+!>(revision)
  ==
++  on-watch
  |=  =path
  ^-  (quip card _this)
  ?:  =(/head path)
    :_  this
    :~  [%give %fact ~ %noun !>(revision)]
        [%give %kick ~ ~]
    ==
  ?>  =(our.bowl src.bowl)
  ?>  ?=([%http-response @ ~] path)
  [~ this]
++  on-agent
  |=  [=wire =sign:agent:gall]
  ^-  (quip card _this)
  |^
    ?>  ?=([%lookup @ ~] wire)
    =/  rid=@ta  i.t.wire
    =/  req=(unit request)  (~(get by pending) rid)
    ?~  req  [~ this]
    ?+  -.sign  [~ this]
        %watch-ack
      ?~  p.sign  [~ this]
      (finish rid 404 ~)
        %fact
      ?^  revision.u.req  [~ this]
      ?.  =(%noun p.cage.sign)
        (finish rid 502 ~)
      =/  rev=(unit @ud)  ((soft ,@ud) q.q.cage.sign)
      ?~  rev  (finish rid 502 ~)
      ?:  =(0 u.rev)  (finish rid 502 ~)
      =.  pending  (~(put by pending) rid u.req(revision rev))
      :_  this
      :~  [%pass /scry/[rid] %keen | (target ship.u.req u.rev)]
      ==
    ==
  ++  finish
    |=  [rid=@ta status=@ud data=(unit json)]
    ^-  (quip card _this)
    =^  cards  pending  (finish-request pending rid status data)
    [cards this]
  --
++  on-arvo
  |=  [=wire sign=sign-arvo]
  ^-  (quip card _this)
  |^
    ?:  =(/bind wire)
      ?>  ?=(%bound +<.sign)
      ?>  accepted.sign
      [~ this]
    ?>  ?=([@ @ ~] wire)
    =/  rid=@ta  i.t.wire
    =/  req=(unit request)  (~(get by pending) rid)
    ?~  req  [~ this]
    ?:  =(%timeout i.wire)
      ?>  ?=([%behn %wake *] sign)
      (finish rid 504 ~)
    ?>  =(%scry i.wire)
    ?>  ?=([%ames %sage *] sign)
    =/  result=sage:mess:ames  +>.sign
    ?~  revision.u.req  (finish rid 502 ~)
    ?.  =((target ship.u.req u.revision.u.req) p.result)
      (finish rid 502 ~)
    ?~  q.result  (finish rid 404 ~)
    ?.  =(%nockster-address p.q.result)
      (finish rid 502 ~)
    =/  value=(unit address:nockster-social)
      ((soft address:nockster-social) q.q.result)
    ?~  value  (finish rid 502 ~)
    ?.  =(ship.u.req ship.u.value)
      (finish rid 502 ~)
    ?:  &(?=(^ address.u.value) !(valid-address u.address.u.value))
      (finish rid 502 ~)
    (finish rid 200 `(record ship.u.value address.u.value))
  ++  finish
    |=  [rid=@ta status=@ud data=(unit json)]
    ^-  (quip card _this)
    =^  cards  pending  (finish-request pending rid status data)
    [cards this]
  --
++  on-leave
  |=  =path
  (on-leave:def path)
++  on-fail
  |=  [=term =tang]
  (on-fail:def term tang)
--
