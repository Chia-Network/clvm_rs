module Bus exposing (Msg (..), noop)

type Msg
    = Noop
    | Compiled Int (Result String String)
    | Ran Int (Result String (String, String))
    | ChangeSource String
    | ChangeArgs String

noop : a -> Msg
noop _ = Noop
