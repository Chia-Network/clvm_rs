module Model.Types exposing
    ( Model
    , CompileRequest
    , RunRequest
    , ExternalInterface
    )

import Platform exposing (Task (..))

type alias CompileRequest =
    { id : Int
    , source : String
    }

type alias RunRequest =
    { id : Int
    , prog : String
    , args : String
    }

type alias Model =
    { compileRequest : Maybe CompileRequest
    , runRequest : Maybe RunRequest
    , source : String
    , sid : Int
    , args : String
    , aid : Int
    , hexProgram : Maybe (Result String String)
    , runResult : Maybe (Result String (String, String))
    }

type alias ExternalInterface msg =
    { requestCompileClvm : Int -> String -> Cmd msg
    , requestRunClvm : Int -> String -> String -> Cmd msg
    }
