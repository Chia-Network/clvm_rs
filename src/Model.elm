module Model exposing (init, update)

import Return as Return
import Task exposing (Task)
import Task

import Bus exposing (Msg (..), noop)
import Model.Types exposing (ExternalInterface, Model)

init () =
    { compileRequest = Nothing
    , runRequest = Nothing
    , source = "()"
    , sid = 0
    , args = "()"
    , aid = 0
    , hexProgram = Nothing
    , runResult = Nothing
    }

updateFieldFromExternal
    : (model -> Maybe comp)
    -> (comp -> idt)
    -> (res -> model -> (model, Cmd msg))
    -> idt -> res -> model -> (model, Cmd msg)
updateFieldFromExternal getPrev getId updateModel id res model =
    getPrev model
        |> Maybe.andThen
           (\req ->
                if getId req /= id then
                    Nothing
                else
                    Just res
           )
        |> Maybe.map (\r -> updateModel r model)
        |> Maybe.withDefault (Return.singleton model)

rerun : ExternalInterface Msg -> Model -> (Model, Cmd Msg)
rerun sys model =
    if Just model.sid /= (model.compileRequest |> Maybe.map .id) then
        -- Needs recompile
        let
            updatedModel =
                { model
                | compileRequest =
                      Just
                          { id = model.sid
                          , source = model.source
                          }
                }
        in
        ( updatedModel
        , sys.requestCompileClvm model.sid model.source
        )
    else
        case model.hexProgram of
            Just (Ok prog) ->
                let
                    newProgram =
                        Just prog /= (model.runRequest |> Maybe.map .prog)
                    newArgs = Debug.log "newArgs" <|
                        Just model.aid /= (model.runRequest |> Maybe.map .id)
                in
                if newProgram || newArgs then
                    let
                        updatedModel =
                            { model
                            | runRequest =
                                  Just
                                      { id = model.aid
                                      , prog = prog
                                      , args = model.args
                                      }
                            }
                    in
                    ( updatedModel
                    , sys.requestRunClvm model.aid prog model.args
                    )
                else
                    Return.singleton model

            _ -> Return.singleton model

update : ExternalInterface Msg -> Msg -> Model -> (Model, Cmd Msg)
update sys msg model =
    case msg of
        Noop -> Return.singleton model

        ChangeSource s ->
            Return.singleton
                { model | source = s, sid = model.sid + 1 }
            |> Return.andThen (rerun sys)

        ChangeArgs a ->
            Return.singleton
                { model | args = a, aid = model.aid + 1 }
            |> Return.andThen (rerun sys)

        Compiled id res ->
            if Just model.sid == (model.compileRequest |> Maybe.map .id) then
                Return.singleton { model | hexProgram = Just res }
                |> Return.andThen (rerun sys)
            else
                rerun sys model

        Ran id res ->
            if Just model.aid == (model.runRequest |> Maybe.map .id) then
                Return.singleton { model | runResult = Just res }
                |> Return.andThen (rerun sys)
            else
                rerun sys model
