module View exposing (view)

import Html exposing (Html, input, div, text, h1)
import Html.Attributes exposing (class, value)
import Html.Events exposing (onClick, onInput)

import Bus exposing (Msg (..))
import Model.Types exposing (Model)

resultToOutput : Maybe (Result String String) -> Html Msg
resultToOutput hp =
    case hp of
        Just (Ok s) -> div [class "output-good"] [text s]
        Just (Err e) -> div [class "output-bad"] [text e]
        Nothing -> div [class "output-missing"] [text "<not present>"]

viewBody model =
    let
        comp = resultToOutput model.hexProgram
        run =
            case model.runResult of
                Just (Ok (hex,decoded)) ->
                    [ div [class "output-segment"]
                          [ div [class "output-label"] [text "Serialized"]
                          , div [class "output-value output-good"] [text hex]
                          ]
                    , div [class "output-segment"]
                        [ div [class "output-label"] [text "Decoded"]
                        , div [class "output-value output-good"] [text decoded]
                        ]
                    ]

                Just (Err e) ->
                    [ div [class "output-segment"]
                          [ div [class "output-label"] [text "Runtime Error"]
                          , div [class "output-value output-bad"] [text e]
                          ]
                    ]

                Nothing ->
                    [ div [class "output-segment"]
                          [ div [class "output-label"] [text "Result"]
                          , div [class "output-missing"] [text "<not run>"]
                          ]
                    ]
    in
    div [class "view"]
        [ div [class "heading"]
              [ h1 [] [ text "Rust CLVM Demo" ] ]
        , div [class "doc"]
            [ div [class "input-bar"]
                  [ div [class "input-entry"]
                        [ div [class "input-label"] [text "CLVM Input:"]
                        , input [ value model.source, onInput ChangeSource ] []
                        ]
                  , div [class "input-entry"]
                      [ div [class "input-label"] [text "Arguments:"]
                      , input [ value model.args, onInput ChangeArgs ] []
                      ]
                  ]
            , div [class "output-bar"]
                (List.concat
                     [ [ div [class "output-segment"]
                             [ div [class "output-label"] [text "Serialized"]
                             , div [class "output-value"] [comp]
                             ]
                       ]
                     , run
                     ]
                )
            ]
        ]

view model =
    { title = "CLVM Rust Code Runner"
    , body = [viewBody model]
    }
