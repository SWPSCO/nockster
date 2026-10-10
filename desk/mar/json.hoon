=,  format
=,  html
|_  jon=^json
++  grow
  |%
  ++  mime  [/application/json (as-octs:mimes -:txt)]
  ++  txt   [(en:json jon)]~
  --
++  grab
  |%
  ++  noun  ^json
  ++  mime  |=([p=mite q=octs] (fall (de:json (@t q.q)) *^json))
  --
++  grad  %mime
--
