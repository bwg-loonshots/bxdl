# NIGO 엔진 연동 계약 계획

- 상태: NIGO의 범위 수용 회신 이후 2026-09-17에 PROPOSED 실행·상태 계약과 실제 개발 후보 JAR/manifest를 확인했다. 양측 상세 계약 확정·정식 공급·BXDL 제품 인수 완료는 아니며 REQ-0002는 `OPEN`이다. 이번 소비자 실행 결과는 별도 results와 [구현 상태](../docs/implementation-status.md)에 기록한다.
- 연결: [구현 계획](./2026-09-16-implementation-plan.md), [인수 계획](./2026-09-16-acceptance-and-release.md).
- 최초 source 검토 기준: `a48d1adef4e9c98dbbb675ec3ff73eee25d00dfb`, 당시 checkout `17b46c151aa2d848b34364ddc43df9c1a202ee41`. 아래 현행 source 분석은 그 기준의 기록이다.
- 2026-09-17 회신 수신: NIGO 문서 commit `87f473deb22dd32f482ffb8f0889d35647e7287e`, 읽은 HEAD `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27`, `requirements/` clean. [BXDL 소비자 회신](./2026-09-17-nigo-feedback-response.md)에 수용 내용·owner·대기 항목을 기록했다. 문서 revision은 engine 공급 revision이 아니다.
- 후속 개발 후보 수신: 읽은 HEAD `17c0bc3b63756915c18fa2942afdd73426bb2eac`(NIGO PR #117), JAR source/contract revision `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27`, `dirty=true`. [원문 스냅샷과 provenance](../contracts/nigo/development-2026-09-17/README.md)에 source·JAR entry·manifest 교차 확인과 남은 조건을 보존했다. 아래 최초 분석과 새 공급 상태를 구분한다.
- 2026-09-17 사용자 결정: 소비자 구현은 Rust, 설치·운용 UX와 실제 package 인수는 macOS Apple Silicon을 먼저 대상으로 한다. Linux/systemd는 후속 서버 profile로 유지한다. [최신 결정](./2026-09-17-rust-macos-first.md)이 2026-09-16 Linux 우선 계획의 실행 순서를 갱신한다.
- 최초 제품 인수는 G1-M(macOS 로컬 운영), 이후 G1-L(Linux 서버·multi-host)로 구분한다. Rust CLI 실행·fixture 통과와 기존 NIGO Mac 실행 근거는 공식 공급물·BXDL 설치/엔진 인수를 대신하지 않는다.

## 1. 최초 source 검토 경계 — 2026-09-16 기준

| 경계 | 현행 기능 | 제품 소비 시 한계 |
| --- | --- | --- |
| canonical bootJar | 검증한 React console 포함 | 공식 공급 manifest·출처·build identity 계약 보강 필요 |
| `GET /monitor/api/console/bootstrap` | instance/chain/genesis/backend·console capability | clientVersion은 고정 문자열. build identity가 아님 |
| `GET /monitor/api/consensus/health` | DB/network와 독립적인 local runtime 관측 | HTTP 200은 조회 성공. 내부 QBFT 타입 노출 존재 |
| `GET /monitor/api/consensus/progress` | pending/vote/finality 근거, IDLE/UNKNOWN 등 | 전역 quorum 증명이 아니며 원자적 ledger snapshot 아님 |
| `POST /rpc` | Native/ETH JSON-RPC | 운영자/API 사용자 인증이 제공된 것은 아님 |
| GC API·job | opt-in 로컬 수동 GC와 제한된 job/command 기록 | 사용자 actor가 로컬 비인증 값으로 고정, 기업 감사 원장 아님 |
| `StorageIntegrityTool` | 별도 JVM, JSON·exit code, 제한된 commitment repair | 문서 진입점은 Gradle. inspect도 DB open/WAL recovery 가능 |

기존 endpoint를 무조건 새 version으로 교체하도록 요청하지 않는다. 먼저 정확한 DTO·field·nullable·시각·오류 의미를 고정한다. 계약 확장이 필요한 항목만 NIGO에 요청하고 BXDL은 adapter에서 소비한다.

### 2026-09-17 개발 후보에서 추가된 경계

- `engine-info`, `preflight`, `init`, `resume-init`, `run`의 명시 Java JAR 명령과 제한된 node/공개 chain JSON이 구현됐다. 실제 표기와 실패 의미는 [실행 계약 원문](../contracts/nigo/development-2026-09-17/ENGINE_CONTRACT.md)을 따른다. legacy Spring 시작과 구분한다.
- Canonical JAR의 `engine-build.json`, 외부 manifest/checksum과 계약 4개 입력의 hash/fingerprint가 제공된다. Bootstrap build metadata, 외부 health DTO와 역할별 local readiness도 추가됐다. 초기 표의 고정 clientVersion·내부 DTO 노출은 최초 기준의 제약이며 새 후보의 설명이 아니다.
- 실제 후보 JAR는 159,965,726 bytes, SHA-256 `37070fbddaf1350b81952b9f3bd718b76f246557d7cd5abe2ec580a13d8de5fe`다. 내장 identity·계약 bytes와 manifest는 읽기 전용으로 대조했다. 현재 HEAD와 다른 dirty source identity를 clean 공급으로 승격하지 않는다.
- 공급자 exact-JAR 시험 근거가 존재하지만 QBFT 전체 종료 증명·live follower source 단절/재선택·초기 sync 경계는 남아 있다. `UNKNOWN/QBFT_CLEANUP_NOT_FULLY_OBSERVED`와 local `READY`/sync `UNKNOWN`을 그대로 소비한다. [상태 계약 원문](../contracts/nigo/development-2026-09-17/ENGINE_RUNTIME_CONTRACT.md)을 따른다.
- 최소 소비자 연결은 pin/identity 확인과 cold 검사다. `engine-info`는 exit 0이어도 `AVAILABLE`을 확인해야 하고, cold의 정상 정적 검사 결과는 exit 3/`INCOMPLETE`다. 실제 init/run/stop·launchd와 G1-M은 이 연결만으로 완료되지 않는다.

## 2. NIGO-01~06 요청안과 우선순위

아래 ID는 기존 NIGO 설계의 범위 식별자를 재사용한다. 새 `REQ-NNNN`을 임의로 예약하거나 기존 요청 상태를 변경하지 않는다.

| ID | 제공 owner / 실제 요청 결과 | BXDL 소비·인수 | 필요 시점 |
| --- | --- | --- | --- |
| NIGO-01 | NIGO: canonical JAR, immutable 식별, engine commit·dirty 여부·JAR hash·console fingerprint·Java/native/platform·계약 fixture revision manifest | BX-011/014: trusted lock에 exact artifact 고정, manifest/실제 bytes 불일치 거부 | 실제 Mac package 및 G1-M 필수; G1-L에서도 재사용·조합 인수 |
| NIGO-02 | NIGO: 공개 chain description과 node-local/secret 분리, canonical 검증, 명시 fresh init/existing restart, missing target의 fresh fallback 거부 | BX-020~022: render·identity 기록. 잘못된 DB/chain/key/partial init 거부 | Mac init/start와 G1-M 필수; OS와 무관한 엔진 안전 계약 |
| NIGO-03 | NIGO: full Spring/DB open/network/signing 없는 cold 검사, JSON·reasonCode·NOT_CHECKED | BX-023: host 검사와 engine 검사 분리, 무변경·미검사 판정 검증 | 실제 preflight 및 G1-M 필수; Mac 로컬 정적 검사만으로 대체 불가 |
| NIGO-04 | NIGO: 외부 DTO·build/capability·role별 startup/readiness·실패 및 종료 계약, secret-free startup report | BX-024/032: 공통 상태 adapter, engine가 못 뜬 경우도 구조화 오류, 결과 불명 보존 | Mac launchd 연결 및 G1-M 필수; Linux systemd는 G1-L |
| NIGO-05 | NIGO: Gradle 없는 공식 offline 도구 실행 artifact/entry point, 독점 접근·JSON·exit code | BX-051/052: inspect/repair wrapper, Node 동시 실행 거부, postCommitAudit 소비 | 후속 유지보수. G1-M/G1-L의 기본 설치·실행 선행 아님 |
| NIGO-06 | NIGO: 위 제공물의 명세·fixture·supplier tests와 exact revision/hash 연결. 엔진의 file H2/RocksDB·QBFT 증거 구분 | BX-040/041: Mac arm64/RocksDB exact package와 로컬 4-validator 회귀를 먼저 인수, Linux multi-host는 별도 후속. H2는 첫 제품 후보 범위 밖 | G1-M에는 01~04 subset; G1-L은 Linux 조합 추가, 유지보수에 05 추가 |

NIGO-01~04/06은 NIGO `requirements/REQ-0002-bxdl-engine-foundation.md`의 `OPEN` 요구로 추적한다. NIGO-05는 같은 상위 주제에서 제공·인수 시점을 분리한다. NIGO 엔진 연동 기반 작업 세션이 방향·범위를 수용했고 BXDL Rust 패키지·Mac 설치/운용 UX 구현 세션이 수신·소비자 인수를 맡는다. 실제 API·flag·schema·fixture의 개발 후보 명세는 이제 존재하며, 양측 확정·잔여 closure·clean 공급 일정과 전체 제품 인수는 대기다. 공유 중인 NIGO 원장은 이 BXDL 변경으로 수정하지 않는다.

## 3. Artifact·config 계약의 최소 내용

아래는 최초 요청에서 정한 최소 의미다. 정확한 개발 후보 field spelling은 날짜별 공급자 원문 스냅샷을 따르며, 양측 확정·지원 완료 여부와 구분한다.

- Artifact: 엔진 source revision, dirty 여부, asset hash, console identity, Java 조건, native 요구, 지원/실제 검증 matrix, 계약 version, 공급자 evidence.
- Chain: canonical genesis/profile/validator identity와 공개 설정, shared와 node-local 항목 구분. chain ID 단독으로 identity를 만들지 않는다.
- Instance: role, node/validator 공개 identity, backend·경로·peer/TLS/secret 참조. 키 원문을 manifest에 넣지 않는다.
- Init: 로컬 신규 data 초기화만 수행하는 범위, 기존/부분 data 거부, 생성 identity 반환, 중단 후 판정 절차. 기존 네트워크에 가입할 때 새 네트워크 genesis를 임의 생성하지 않는다.
- Restart: 기존 DB/WAL·key·profile을 검증하고 누락·불일치에서 중단. fresh DB fallback 없음. existing restart의 경로 거부는 엔진의 DB open·schema 등록 전부터 적용하며 BXDL 파일 존재 검사로 대체하지 않는다.
- Compatibility: source→target engine·DB/profile 조합별 허용/거부와 실제 증거. 미제공이면 upgrade 미지원이며 자동 migration/downgrade 없음.

engine lock과 NIGO manifest는 공급 산출물을 식별한다. BXDL package manifest는 제품 CLI·runtime·설정 schema·engine lock·payload hash를 함께 식별한다. 두 manifest는 소유권이 다르며 제품이 엔진의 검증 이력을 만들어 채우지 않는다.

이번 NIGO node JSON의 root는 `chainFile`, `dataDirectory`, `backend`, `node`다. BXDL instance JSON을 그대로 전달할 수 없고 기존 제품 필드만으로 validator identity·peer·mTLS pin·sync 정책을 추측할 수 없다. 첫 cold adapter는 명시적으로 제공한 NIGO config를 소비하며, 제품 설정 rendering은 필요한 canonical mapping을 별도로 확보한다. `chainFingerprint`는 정렬된 공개 chain property map의 JSON hash이며 원본 파일 SHA-256과 다르다.

개발 manifest의 `darwin/arm64`와 기존 `linux/amd64`는 대상 schema다. 동일 JAR를 소비하더라도 CLI·JRE/native·환경별 hash와 실제 실행 인수는 각각 연결한다. 선정 Mac JRE와 launchd를 포함한 G1-M을 먼저 검증하고, 이를 Linux/native·systemd·multi-host G1-L의 PASS로 재사용하지 않는다.

## 4. 초기화·검사·종료의 부수 효과

| 작업 | 허용 효과 | 반드시 보존할 실패 의미 |
| --- | --- | --- |
| cold preflight | 기존 config/key의 읽기·공개 identity 도출, host 정적 관측 | DB 내부 상태는 NOT_CHECKED. DB open·schema 생성·listen·송신·signing·키 생성 없음 |
| explicit init | 확인한 새 instance의 데이터 생성; 해당 profile의 engine 계정으로 공식 도구 실행(Mac 사용자 / 후속 Linux 서비스 UID) | 기존 DB overwrite 금지. partial init은 보존하고 engine 판정 없이는 재실행 금지 |
| existing start | 기존 데이터 검증, 허용된 WAL recovery, 네트워크 시작 | 경로 오타·profile/key 불일치에서 새 chain 생성 금지 |
| graceful stop | timer/network/signing 중지와 storage close | timeout, fail-stop, durability 결과 불명은 정상 종료와 구분 |
| offline inspect | 기존 DB exclusive open, 검사; backend에 따라 물리 복구/쓰기 가능 | 무변경 검사로 표기하지 않음. 대상 없음/lock 충돌/close 오류 거부·보고 |
| limited repair | 엔진이 정의한 commitment 범위 수정 | 이미 commit됐을 가능성과 postCommitAudit 실패를 보존. 자동 재시도 없음 |

Legacy Spring 기동은 schema를 기록하고 빈 DB에 genesis를 생성할 수 있다. `auto-start=false`, H2 durability preflight, RocksDB `openExisting`을 cold 검사로 재사용하지 않는다. 새 개발 후보는 별도 `preflight` 진입점을 제공하며 DB/WAL·network·native를 `NOT_CHECKED`로 반환한다. 정적 검사 뒤에도 engine startup 검증과 독점 접근이 필요하다.

init 성공 후 BXDL manifest 기록 전에 중단되는 경우를 NIGO-02에 포함한다. 데이터 존재만으로 성공이나 실패를 단정하지 않는다. NIGO가 DB/chain/key·합의 안전 상태를 판정하고 BXDL이 제품 기록의 재연결 여부를 결정한다. 제품 기록 부재만으로 DB를 다시 초기화하지 않는다. 새 후보의 `resume-init`은 INITIALIZING journal과 기존 ledger를 요구하는 명시 변경 작업이며 cold 검사와 다르다. genesis가 없는 부분 초기화는 보존·거부한다. BXDL의 해당 adapter와 인수가 끝나기 전에는 초기화 불명 상태로 start를 막는다.

## 5. 관측·오류 계약

외부 DTO의 nullable, unknown enum, 큰 정수 표현, wall-clock/monotonic 의미, 관측 시각·instance reset 규칙을 fixture로 제공받는다. 엔진 내부 Java class 이름이나 로그 문장 parsing을 장기 계약으로 사용하지 않는다.

BXDL이 필요한 관측은 process alive, local engine initialization, role별 readiness, sync 상태, 관측 가능한 finality 근거다. 이 항목을 단일 healthy boolean으로 압축하지 않는다. validator/observer/manual-start·INSTANT를 구분하고 observer의 `running=false`·`NOT_APPLICABLE`이나 무거래 `IDLE`·`UNKNOWN`만으로 장애를 표시하지 않는다.

개발 후보는 stdout command JSON과 새 파일의 append-only JSONL process report를 제공한다. report의 `attemptId`, `command`, `sequence`, `pid`, `observedAt`, `status`, `reason`, `contractStatus`, `details`를 정확한 실행 시도와 연결하며 미완성 마지막 행·이전 실행의 stale 보고를 완료 근거로 쓰지 않는다. 상세 의미는 공급자 실행 계약을 따른다. init/resume-init은 exit 0·stdout·동일 attempt의 마지막 완전한 report가 일치해야 하고, 보고 부재는 결과 불명이다. BXDL의 process/service adapter 인수는 별도다.

종료 계약은 최소한 process 종료와 storage/safety 결과를 구분해야 한다. 필요한 작업 정리와 storage close의 엔진 근거를 서비스 관측과 조합하며 보고 유실·timeout은 결과 불명이다. SIGTERM·process 소멸만으로 정상 close를 선언하지 않고 강제 종료에서 불가능한 성공 보고를 요구하지 않는다. 원장 상태를 직접 읽어 BXDL이 안전 종료를 추측하지 않는다. 도구가 crash하거나 JSON을 끝내지 못하면 exit code만으로 transaction 성공/실패를 확정하지 않는다.

launchd job 제거, systemd stop 응답, 컨테이너 종료는 각각의 플랫폼 관측이다. 공통 엔진 DTO에 unit/InvocationID/cgroup을 필수로 요구하지 않는다. Mac의 로그아웃·sleep/wake·종료 timeout과 Linux의 서비스·재부팅 조건은 별도 adapter 인수에 남기며, 엔진 종료 계약은 공통으로 소비한다.

## 6. 계약 fixture·버전 관리

`contracts/nigo/development-2026-09-17/`에 공급자 계약 원문 4개와 실제 개발 후보 manifest, BXDL provenance README를 저장했다. 원문 bytes를 편집하지 않고 최초 채취 시 source·JAR entry·manifest hash를 교차 확인했다. 이후 같은 HEAD의 source 계약 미커밋 변경은 README에 별도로 기록하고, 후보 JAR와 일치하는 snapshot을 보존했다. 현재 수준은 `PROPOSED`이며 파일 보관만으로 `SUPPLIER_DELIVERED`/`CONSUMER_VERIFIED`로 바꾸지 않는다. 실제 임의 고객 응답·secret을 fixture로 commit하지 않는다.

처음에는 고정 revision adapter를 사용한다. 미지원 계약은 명시 거부하며 필수 field 누락을 default 정상값으로 메우지 않는다. schema version 변화 없는 의미 변경도 회귀로 검출하도록 golden fixture와 실제 artifact contract test를 함께 둔다. protocol/DB schema version을 제품 계약 version과 혼동하지 않는다.

Supplier test의 source 회귀와 exact-JAR 회귀를 구분하고, consumer test는 제공받은 JAR를 BXDL이 선택한 runtime/package로 별도 검증한다. NIGO managed E2E의 fresh build를 BXDL package gate에서 실행하지 않는다. 새 엔진 조합을 채택할 때 lock 변경·adapter 영향·실제 인수 결과를 같은 변경에 연결한다.

node별 hash/root는 동일한 확정 블록 높이에서 비교한다. 각 node의 latest 관측을 원자적 snapshot으로 취급하지 않는다. 동일 DB/key/WAL 재시작은 원장·키·합의 안전 상태를 보존한 재시작이며 정상 실행 중 변하는 DB/WAL의 물리 byte 불변을 요구하지 않는다.

## 7. 요구 전달 절차와 미합의 항목

1. NIGO `requirements/README.md`와 요청 원문의 최신 상태를 확인한다. REQ-0001은 문서 인계, REQ-0002는 기능·산출물 요구다.
2. 수신한 문서 revision·범위·의미·owner와 BXDL 기반 revision을 [소비자 회신](./2026-09-17-nigo-feedback-response.md)에 기록한다. 피드백 4·5절의 완료 확인·공급 제출·결과 교환 절차를 따른다.
3. NIGO owner가 실제 호출·schema·fixture를 제안하면 BXDL이 adapter/인수 가능 여부를 회신한다. 상세 계약·완료 기준 양측 확정 전에는 상태를 임의 전환하지 않는다.
4. NIGO owner가 원문 7절에 실제 artifact·fixture·공급자 실행 근거·제한을 제출하면 BXDL이 exact package/JRE와 test-owned 환경으로 해당 A1~A6을 인수한다. NIGO-05/A7은 후속이다.
5. BXDL 실제 결과를 공유 가능한 revision으로 회신하고 NIGO 원문 7·8절의 근거와 연결한다. `DELIVERED`와 `VERIFIED`, source regression과 package acceptance를 구분하며 G1-M만으로 Linux/A7까지 완료 처리하지 않는다.

현재 남은 항목: 제공된 PROPOSED 호출·manifest·DTO·report 계약의 양측 확정과 소비자 검증, QBFT 종료·live follower·초기 sync closure, clean/정식 공급 일정, compatibility 범위, Mac 및 후속 Linux JRE 공급자/patch, 최소 macOS·native 조합, 정식 signed package trust root. REQ-0002는 `OPEN`이다. R1 setup 이후 정적 설치와 후보 identity/cold adapter를 다음 구현으로 연결하며 최신 구현·실제 실행 결과는 [구현 상태](../docs/implementation-status.md)를 따른다. 엔진 init/start/stop·launchd와 G1-M 전체를 개발용 조립·cold 확인만으로 완료 처리하지 않는다.

원격 운영은 별도 계약이다. 현재 P2P mTLS를 운영자 로그인으로 간주하지 않는다. GC의 고정 local actor를 전달받은 임의 user 문자열로 바꾸는 것만으로 인증을 완성하지 않는다. 인증된 service identity·실제 사용자·engine job/command의 신뢰 가능한 연결과 raw RPC 접근 차단을 함께 설계한다.

## 8. 근거 파일

NIGO 기준 revision의 다음 파일을 참조했다.

- `design/2026-09-16-bxdl-product-distribution-and-engine-contract-design.md`, `requirements/REQ-0001-bxdl-bootstrap-handoff.md`.
- `requirements/REQ-0002-bxdl-engine-foundation.md`, `requirements/REQ-0002-nigo-provider-feedback.md`: 2026-09-17 수신 문서 revision은 상단 기록을 따른다.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/initializer/GenesisBlockInitializer.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/ProtocolApplication.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/consensus/runtime/H2QbftDurabilityBarrier.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/monitor/dto/ConsensusHealthDto.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/monitor/service/MonitorConsoleBootstrapService.java`.
- `nigo-java/nigo-node/NODE_CONFIGURATION.md`, `QBFT_DEVNET_RUNBOOK.md`, `STORAGE_INTEGRITY.md`, `CONSOLE_GC.md`.
- 후속 HEAD `17c0bc3b63756915c18fa2942afdd73426bb2eac`의 `ENGINE_CONTRACT.md`, `ENGINE_RUNTIME_CONTRACT.md`, `engine/contract.json`, `engine-contract/health-cases.json` 및 `tasks/2026-09-17-bxdl-engine-foundation.md`. 원문 스냅샷과 실제 artifact identity는 상단 링크를 따른다.

운영 문서의 시점 차이는 source와 최신 task evidence로 확인한다. 예를 들어 오래된 RocksDB 운영 문구의 GC 미구현 설명을 현재 양 backend 수동 GC 구현 전체의 부재로 해석하지 않는다. 자동 GC·전체 disk bound는 별도 상태다.
