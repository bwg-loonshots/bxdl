# BXDL용 로컬 runtime 관측 계약 초안

REQ-0002 NIGO-04의 HTTP 관측 부분을 구현한 개발 후보 계약이다. 공급자/소비자 합의 또는 G1-M/G1-L
제품 인수 완료가 아니다. 초기화·cold 검사·HTTP 전 startup 실패·HTTP 종료 후 종료 보고는 각각 별도 계약이다.

## 기존 endpoint와 호환성

- `GET /monitor/api/consensus/health`: DB·network·state-machine lock 없이 bounded 메모리 관측.
- `GET /monitor/api/consensus/progress`: 기존 로컬 실행·합의 진행 분류. `IDLE`은 장애가 아니며,
  observer/INSTANT의 `NOT_APPLICABLE`은 validator 평가 대상이 아니라는 뜻이다.
- `GET /monitor/api/console/bootstrap`: 기존 필드를 유지하고 `engine.build`, `engine.capabilities`를 추가한다.
  bootstrap은 ledger 읽기를 포함하므로 최소 health와 독립적으로 실패하거나 지연될 수 있다.

기존 health의 `running`, `lifecycleRunning`, nullable runtime 필드와 failure/execution 필드 이름·값은 유지한다.
`failure`, `executionProgress` 및 그 안의 view는 내부 Java consensus record를 노출하지 않는 외부 DTO다.
기존 rich diagnostics의 event history 전체를 이 계약으로 고정한 것은 아니다.
`readiness`는 추가 필드이므로 이전 consumer는 알 수 없는 필드를 무시할 수 있다.
HTTP 200은 관측 응답 성공일 뿐 실행·합의·저장소 정상이나 운영 권한을 뜻하지 않는다.

## 역할별 로컬 readiness

`readiness` 필드: `status`, `reason`, `role`, `startupMode`, `startupCompleted`, `syncStatus`.
role은 `VALIDATOR`, `OBSERVER`, `INSTANT`, `UNKNOWN`; startupMode는 `AUTO_START`, `MANUAL_START`다.
`startupCompleted`는 lifecycle의 startup gate/activation 반환을 관측한 값이며, DB health 검사를 새로 실행하지 않는다.
따라서 이후 storage 장애를 판정하지 않는다. `syncStatus`는 이 endpoint에서 sync를 검사하지 않으므로 항상 `UNKNOWN`이다.

| 관측 | status / reason |
| --- | --- |
| terminal failure evidence | `FAILED` / `TERMINAL_FAILURE` |
| local operation/timer overdue | `NOT_READY` / `LOCAL_EXECUTION_OVERDUE` |
| startup 미완료 또는 정지 후 | `NOT_READY` / `STARTUP_INCOMPLETE` |
| role 식별 불가 | `UNKNOWN` / `ROLE_UNKNOWN` |
| QBFT runtime 초기화 관측 미완료 | `UNKNOWN` / `RUNTIME_UNINITIALIZED` |
| 초기화된 observer; validator engine 없음도 정상 | `READY` / `OBSERVER_INITIALIZED` |
| auto-start 비활성, 역할의 로컬 startup 완료 | `READY` / `MANUAL_START_INITIALIZED` |
| 자동 시작 validator engine 누락/정지 | `NOT_READY` / `ENGINE_MISSING` 또는 `ENGINE_NOT_RUNNING` |
| 실행 evidence 없음/NOT_STARTED/STOPPED/NO_TIMER, running 표시는 true | `UNKNOWN` / `EXECUTION_UNKNOWN` |
| 자동 시작 validator 실행 관측 | `READY` / `VALIDATOR_RUNNING` |
| INSTANT lifecycle 실행 관측 | `READY` / `INSTANT_RUNNING` |

위 표의 순서대로 우선 적용한다. Observer/manual-start는 기존 `running=false`여도 local `READY`일 수 있다.
`READY`는 caught-up·최근 finality·quorum·peer 연결·validator signing 가능성을 보장하지 않는다.
`NOT_READY`/`UNKNOWN`이나 timeout은 terminal failure, 안전한 재시도 또는 자동 재시작의 허가가 아니다.

## live finality follower의 남은 제한

현재 `HistoricalCatchUpCoordinator`는 startup에서 선택한 단일 source의 live feed를 observer 또는
manual-start follower에 연결한다. startup 후보 선택·fallback과 달리, 지속 follow 중 그 source가 종료되거나
feed가 실패한 뒤에는 다른 source를 자동 재선택하지 않는다.

`ChannelFinalitySyncPeer`의 follow loop는 실패를 callback에 전달하고 feed를 닫지만,
`QbftCatchUpCoordinatorFactory`가 연결하는 현재 live failure callback은 no-op이다. 따라서
`GET /monitor/api/consensus`의 `historicalSync.phase`가 `FOLLOWING`으로 남아 있어도 현재 feed가 살아 있거나
새 finalized block을 적용하고 있다는 보장은 없다. `historicalSync.sourceNodeId`와 각 노드의 실제 head를
함께 확인해야 하며, 이 관측만으로 자동 복구가 수행되었다고 판단하지 않는다.

위 제한은 local `READY` 및 `syncStatus=UNKNOWN`에서도 적용된다. 특히 observer/manual-start의 startup 완료는
follower의 현재 최신성을 뜻하지 않는다. 이는 코드에서 확인한 구현 경계이며, 재현 중인 최초 exact JAR
테스트 실패의 확정 원인으로 단정한 내용은 아니다. 소스 단절 뒤 재선택·최신성 관측은 별도 후속 검증 대상이다.

## 수치·시간·일관성

- `application/json`은 기존 long 숫자를 유지한다. JavaScript 등 정확한 64-bit 처리가 필요한 소비자는
  `Accept: application/vnd.nigo.console+json`을 사용한다. 여기서는 long/Long/BigInteger가 항상 십진 문자열이며
  int·boolean·null은 그대로다. 높이·나노초 등이 2^53을 넘어도 exact 값을 보존한다.
  round의 도메인 제약은 기존 QBFT unsigned 32-bit 범위를 유지한다.
- `observedAtEpochMillis`와 failure의 `startedAtEpochMillis`는 Unix epoch milliseconds다.
  실패의 `startedAtNanos`/`completedAtNanos`는 동일 JVM 내부 monotonic 값으로, 다른 실행/노드와 비교하지 않는다.
  execution의 age/duration은 monotonic 경과시간이며 `nextTimerDueInMillis`만 음수일 수 있다.
- null은 측정 불가/적용 불가이며 0, 성공, 정지를 뜻하지 않는다. failure 없음도 전체 건강 보장이 아니다.
- 각 구성 요소와 endpoint는 독립 표본이다. 시간값은 heartbeat가 아니며 원자적 chain snapshot으로 사용하지 않는다.
  동일 높이의 block hash/state root 비교는 별도로 수행한다.
- 오류는 고정 category만 포함한다. 원문 exception, config, 환경변수, private key를 반환하지 않는다.

## build와 capability

bootstrap의 `clientVersion`은 `EngineBuildInfo` metadata에서 얻으며 metadata가 없거나 잘못되면 unavailable이다.
`engine.build`는 canonical JAR에 포함된 allowlisted build identity이며, resource 부재를 임의 source identity로
대체하지 않는다. JAR 자체 checksum은 외부 공급 manifest에서 확인한다. 자기 자신의 JAR hash를 metadata에 넣지 않는다.

`engine.capabilities`는 `CONSENSUS_HEALTH`, `CONSENSUS_PROGRESS`, `ROLE_AWARE_LOCAL_READINESS`만 광고한다.
이는 해당 관측 형식 구현 여부이며 init/cold 검사/종료 보고/운영자 인증/공식 release 또는 package 지원 승인이 아니다.
console의 `access` 제약은 그대로 유지한다.

## 검증 fixture

[`src/test/resources/engine-contract/health-cases.json`](src/test/resources/engine-contract/health-cases.json)은
validator·observer·manual-start·UNKNOWN·startup 미완료·terminal failure의 exact decimal-string wire fixture다.
`EngineHealthContractFixtureTests`가 실제 MVC 직렬화와 readiness projection을 비교한다.
`ConsensusHealthProjectionTests`는 기존 내부 failure/execution JSON과 외부 DTO의 호환성을 검증한다.
`ConsensusHealthHttpIntegrationTests`는 test-owned file H2/RocksDB 장애 중에도 관측 경로를 검증한다.
이 테스트의 존재나 소스 회귀 통과를 공급 artifact·선정 JRE·BXDL 실제 package 인수 증거로 확대하지 않는다.

## 종료 근거의 남은 제한

이번 보강에서 `QbftNetworkStartupGate`, `HistoricalCatchUpCoordinator`, `QbftPeerNetwork`의 명시 종료는
필수 close 예외를 모아 전파하고 남은 owner의 종료를 계속 시도한다. 실패 뒤 재호출은 성공으로 바뀌지 않는다.
peer worker의 종료 대기 timeout·interruption도 종료 확인 실패다. 기존 startup 실패 경로의 best-effort 정리는
원래 오류를 유지한다. 이 범위의 보강만으로 전체 비동기 작업 정리가 증명되지는 않는다.

INSTANT scheduler는 실행 중인 mining critical section이 끝난 뒤 예정된 callback을 차단하고, mining monitor를
놓은 상태에서 실제 executor 종료를 확인한다. timeout 뒤 `shutdownNow()`를 호출한 경우에도 두 번째 종료 확인이
필요하며 timeout/interruption은 sticky 실패다. 이 경우 재시작·manual mining으로 자동 진행하지 않는다.
정상 stop 후 또는 auto-start 없이 호출하는 명시 `mineNow()`/`mineCycle()`의 기존 manual 동작은 유지한다.

- `ChannelFinalitySyncPeer`의 follower handle은 제한 시간 join 뒤 생존 여부를 확인하지 않는다.
- `BlockSyncServer`의 worker 종료 대기는 반환값으로 실제 종료를 확인하지 않는다.
- `ManagedQbftConsensusEngine`의 signer executor 종료 대기도 실제 종료를 확인하지 않는다.

따라서 QBFT 최상위 stop API 반환·storage close만으로 NIGO-04의 완전한 정상 종료를 선언하지 않는다.
managed report의 구체적인 `UNKNOWN` reason 및 INSTANT 실행 모드별 제한은 [실행 계약](ENGINE_CONTRACT.md)을 따른다.
이 잔여 사항은 전체 NIGO-04 완료 또는 G1-M 인수로 가리지 않고 후속 검증 대상으로 남긴다.
