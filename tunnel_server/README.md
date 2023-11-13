# Serverless

## Connection

```plantuml
title Serverless Tunnel Connection Sequence Diagram

skinparam maxMessageSize 260
skinparam ParticipantPadding 20
skinparam BoxPadding 10

box Local Network
    participant "Local MoosicBox Server\n(moosicbox_server)" as MoosicBox
end box

box Cloud #ffd28f
    box Lambda #ffd28f
        participant "Tunnel WS\n(moosicbox_tunnel_server_ws)" as WS
end box

box PlanetScale #8fc3ff
    database MySQL
end box

MoosicBox -> WS: Connect request
WS -> MoosicBox: Connect success

MoosicBox -> WS: [InitConnection]\n{clientId: "12345"}
MoosicBox -> MySQL: Upsert client_id and tunnel_ws_id in `connections` table

loop each 5 minutes (to keep lamda ws alive)
    MoosicBox -> WS: [Ping]
    WS -> MoosicBox: [Pong]
end
```

## Data Stream Request

```plantuml
title Serverless Tunnel Data Stream Request Sequence Diagram

skinparam maxMessageSize 260
skinparam ParticipantPadding 20
skinparam BoxPadding 10

box Internet
    actor Client #green
end box

box Cloud #ffd28f
    box Lambda #ffd28f
        participant "Tunnel API\n(moosicbox_tunnel_server)" as API
    end box
    box Lambda #ffd28f
        participant "Tunnel WS\n(moosicbox_tunnel_server_ws)" as WS
    end box
end box

box Local Network
    participant "Local MoosicBox Server\n(moosicbox_server)" as MoosicBox
end box

box PlanetScale #8fc3ff
    database MySQL
end box

Client -> API: Request data\n{clientId: "12345"}
API -> WS: Request data\n{clientId: "12345"}
WS -> MySQL: Request tunnel_ws_id for clientId "12345"
activate MySQL#gray
MySQL -> WS: Return tunnel_ws_id "127347374123"
deactivate MySQL#gray
WS -> MoosicBox: Request data from ws connection "127347374123"

loop until all data sent
    MoosicBox -> MoosicBox: Read data (bytes)
    activate MoosicBox#gray
    MoosicBox -> MoosicBox: Encode data (base64)
    deactivate MoosicBox#gray

    MoosicBox -> WS: Send packet of data (base64)
    activate WS#gray
    deactivate WS#gray

    WS -> API: Send packet of data (base64)
    activate API#gray
    deactivate API#gray
    API -> API: Decode data (bytes)
    activate API#gray
    deactivate API#gray

    API -> Client: Send packet of data (bytes)
    activate Client#gray
    deactivate Client#gray
end
```

## Data Blocking Request

```plantuml
title Serverless Tunnel Data Blocking Request Sequence Diagram

skinparam maxMessageSize 260
skinparam ParticipantPadding 20
skinparam BoxPadding 10

box Internet
    actor Client #green
end box

box Cloud #ffd28f
    box Lambda #ffd28f
        participant "Tunnel API\n(moosicbox_tunnel_server)" as API
    end box
    box Lambda #ffd28f
        participant "Tunnel WS\n(moosicbox_tunnel_server_ws)" as WS
    end box
end box

box Local Network
    participant "Local MoosicBox Server\n(moosicbox_server)" as MoosicBox
end box

box PlanetScale #8fc3ff
    database MySQL
end box

Client -> API: Request data\n{clientId: "12345"}
API -> API: Create response buffer
activate API#gray
deactivate API#gray
API -> WS: Request data\n{clientId: "12345"}
WS -> MySQL: Request tunnel_ws_id for clientId "12345"
activate MySQL#gray
MySQL -> WS: Return tunnel_ws_id "127347374123"
deactivate MySQL#gray
WS -> MoosicBox: Request data from ws connection "127347374123"

loop until all data saved to buffer
    MoosicBox -> MoosicBox: Read data (bytes)
    activate MoosicBox#gray
    MoosicBox -> MoosicBox: Encode data (base64)
    deactivate MoosicBox#gray

    MoosicBox -> WS: Send packet of data (base64)
    activate WS#gray
    deactivate WS#gray

    WS -> API: Send packet of data (base64)
    activate API#gray
    deactivate API#gray
    API -> API: Decode data (bytes)
    activate API#gray
    deactivate API#gray
    API -> API: Push bytes to response buffer
    activate API#gray
    deactivate API#gray
end

API -> Client: Send response buffer (bytes)
activate Client#gray
deactivate Client#gray
```

# Server

## Connection

```plantuml
title Server Tunnel Connection Sequence Diagram

skinparam maxMessageSize 260
skinparam ParticipantPadding 20
skinparam BoxPadding 10

box Local Network
    participant "Local MoosicBox Server\n(moosicbox_server)" as MoosicBox
end box

box Cloud #ffd28f
    box "Server\n(moosicbox_tunnel_server)"
        participant "Tunnel WS\n(moosicbox_tunnel_server_ws)" as WS
    end box
end box

box PlanetScale #8fc3ff
    database MySQL
end box

MoosicBox -> WS: Connect request
WS -> MoosicBox: Connect success

MoosicBox -> WS: [InitConnection]\n{clientId: "12345"}
MoosicBox -> MySQL: Upsert client_id and tunnel_ws_id in `connections` table

loop each 5 minutes
    MoosicBox -> WS: [Ping]
    WS -> MoosicBox: [Pong]
end
```

## Data Stream Request

```plantuml
title Server Tunnel Data Stream Request Sequence Diagram

skinparam maxMessageSize 260
skinparam ParticipantPadding 20
skinparam BoxPadding 10

box Internet
    actor Client #green
end box

box Cloud #ffd28f
    box "Server\n(moosicbox_tunnel_server)"
        participant "Tunnel API" as API
        participant "Tunnel WS" as WS
    end box

    box Local Network
        participant "Local MoosicBox Server\n(moosicbox_server)" as MoosicBox
end box

box PlanetScale #8fc3ff
    database MySQL
end box

Client -> API: Request data\n{clientId: "12345"}
API -> WS: Request data\n{clientId: "12345"}
WS -> MySQL: Request tunnel_ws_id for clientId "12345"
activate MySQL#gray
MySQL -> WS: Return tunnel_ws_id "127347374123"
deactivate MySQL#gray
WS -> MoosicBox: Request data from ws connection "127347374123"

loop until all data sent
    MoosicBox -> MoosicBox: Read data (bytes)
    activate MoosicBox#gray
    deactivate MoosicBox#gray

    MoosicBox -> WS: Send packet of data (bytes)
    activate WS#gray
    deactivate WS#gray

    WS -> API: Send packet of data (bytes)
    activate API#gray
    deactivate API#gray

    API -> Client: Send packet of data (bytes)
    activate Client#gray
    deactivate Client#gray
end
```

## Data Blocking Request

```plantuml
title Server Tunnel Data Blocking Request Sequence Diagram

skinparam maxMessageSize 260
skinparam ParticipantPadding 20
skinparam BoxPadding 10

box Internet
    actor Client #green
end box

box Cloud #ffd28f
    box "Server\n(moosicbox_tunnel_server)"
        participant "Tunnel API" as API
        participant "Tunnel WS" as WS
    end box

    box Local Network
        participant "Local MoosicBox Server\n(moosicbox_server)" as MoosicBox
end box

box PlanetScale #8fc3ff
    database MySQL
end box

Client -> API: Request data\n{clientId: "12345"}
API -> API: Create response buffer
activate API#gray
deactivate API#gray
API -> WS: Request data\n{clientId: "12345"}
WS -> MySQL: Request tunnel_ws_id for clientId "12345"
activate MySQL#gray
MySQL -> WS: Return tunnel_ws_id "127347374123"
deactivate MySQL#gray
WS -> MoosicBox: Request data from ws connection "127347374123"

loop until all data saved to buffer
    MoosicBox -> MoosicBox: Read data (bytes)
    activate MoosicBox#gray
    deactivate MoosicBox#gray

    MoosicBox -> WS: Send packet of data (bytes)
    activate WS#gray
    deactivate WS#gray

    WS -> API: Send packet of data (bytes)
    activate API#gray
    deactivate API#gray
    API -> API: Push bytes to response buffer
    activate API#gray
    deactivate API#gray
end

API -> Client: Send response buffer (bytes)
activate Client#gray
deactivate Client#gray
```
