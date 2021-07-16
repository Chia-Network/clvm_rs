port module Main exposing (main)

import Browser

import Bus exposing (Msg (..))
import Model.Types exposing (Model, ExternalInterface)
import Model exposing (init, update)
import View exposing (view)

encodeExtPairResult
    : (Int -> Result String p -> Msg)
    -> (Int,String,p)
    -> Msg
encodeExtPairResult msgTag (id,error,success) =
    if error /= "" then
        msgTag id (Err error)
    else
        msgTag id (Ok success)

main : Program () Model Msg
main =
    Browser.document
        { init = \flags -> (init (), Cmd.none)
        , view = view
        , update =
            update
                { requestCompileClvm = \id source ->
                      requestCompileClvm (id,source)
                , requestRunClvm = \id source args ->
                      requestRunClvm (id,source,args)
                }
        , subscriptions = \_ ->
            Sub.batch
                [ respondCompileClvm (encodeExtPairResult Compiled)
                , respondRunClvm (encodeExtPairResult Ran)
                ]
        }

port requestCompileClvm : (Int, String) -> Cmd msg
port requestRunClvm : (Int, String, String) -> Cmd msg

port respondCompileClvm : ((Int, String, String) -> msg) -> Sub msg
port respondRunClvm : ((Int, String, (String, String)) -> msg) -> Sub msg
