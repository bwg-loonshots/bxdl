# REQ-0002 제공자 피드백에 대한 BXDL 소비자 회신

- 작성일: 2026-09-17
- 결론: NIGO의 범위 수용 회신을 확인했다. 피드백 3절의 안전·인수 의미와 4·5절의 완료 확인·결과 교환 절차를 BXDL 소비자 관점에서 수용하며 이견은 없다.
- 상태: **범위·의미·교환 절차의 수신 및 수용 기록**이다. 최초 회신 후 `PROPOSED` 계약·fixture와 development JAR/manifest를 수신했다. 정확한 수신 identity와 한계는 7절에 추가했으며, 실제 G1-M 인수 완료나 REQ-0002 전체 종료를 뜻하지 않는다. `OPEN`을 이 회신만으로 변경하지 않는다.
- 연결: [엔진 연동 계약](./2026-09-16-engine-integration.md), [Mac 우선 결정](./2026-09-17-rust-macos-first.md), [제품 인수 계획](./2026-09-16-acceptance-and-release.md), [구현 상태](../docs/implementation-status.md).

## 1. 최초 회신 시점의 수신 revision과 owner

| 항목 | 확인한 값 |
| --- | --- |
| 제공자 문서 | NIGO `requirements/REQ-0002-nigo-provider-feedback.md`, 2026-09-17판 전체 |
| 문서 변경 commit | `87f473deb22dd32f482ffb8f0889d35647e7287e` |
| 읽은 NIGO checkout HEAD | `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27` |
| 읽기 시 요구 디렉터리 상태 | `requirements/`의 미커밋 변경 없음 |
| NIGO 제공 owner | NIGO 엔진 연동 기반 작업 세션. 엔진 구현·회귀·공급의 실행 책임은 해당 owner에게 있음 |
| BXDL 수신·소비자 인수 owner | BXDL Rust 패키지·Mac 설치/운용 UX 구현 세션 |
| 공유 가능한 BXDL 기반 | `a3a18d531cc4f0368559a6f1090724fcd21cbf8c` — PR #1 병합 기반 |
| 이번 회신의 공유 revision | 이 문서를 포함한 후속 commit/PR을 공유할 때 별도 기록. 위 기반 commit이 이번 회신을 포함한다는 뜻은 아님 |
| 실제 engine/package 인수 | 미실행. 위 문서·제품 기반 commit은 공급 artifact의 source/hash 또는 제품 인수 결과가 아님 |

NIGO 요청 원문 `requirements/REQ-0002-bxdl-engine-foundation.md`와 상태 규칙도 함께 확인했다. 위 표는 최초 회신 당시의 읽기·공유 기반이며, 이후 후보 공급은 7절의 별도 identity를 따른다. NIGO 요구 원장은 다른 세션과 공유 중이며 이번 작업에서는 수정하지 않았다. 이 문서의 공유 revision을 전달한 뒤 NIGO owner가 자신의 원문·인덱스·응답 로그와 대조할 수 있다.

## 2. 수용 범위

| 범위 | BXDL 회신 |
| --- | --- |
| NIGO-01~04/06 | 첫 Mac arm64·Java 21·RocksDB의 엔진 연동 기반 묶음으로 수용. artifact identity, 명시 init/restart, cold 검사, 역할별 상태·기동/종료 결과, fixture·공급자 근거를 실제 소비할 때 연결 |
| Linux G1-L | 선정 Linux package·systemd·native·multi-host를 후속 별도 인수. Mac 성공을 Linux 결과로 복사하지 않음 |
| NIGO-05/A7 | 유지보수 후속으로 수용. 첫 G1-M 설치·기동의 선행으로 추가하지 않음 |
| 기존 설계와 비목표 | Rust는 BXDL 구현 언어이며 NIGO Java 엔진 재작성 요구가 아님. 공급자의 file H2/RocksDB 회귀, 기존 bootstrap/health/progress 재사용, 불필요한 DB schema·genesis/profile·API version 추가 금지를 유지 |
| 책임 경계 | NIGO는 canonical chain/key·DB/WAL·합의 안전 판단과 공식 실행·보고 계약을 소유. BXDL은 제품 입력·작업 기록·서비스 관측·지정 package의 소비자 인수를 담당 |

이 수용은 NIGO owner의 구현 일정이나 실제 실행 시작을 대신 확정하지 않는다. 작은 기능마다 별도 사용자 승인·PR을 요구하는 절차로 해석하지 않으며, 각 세션은 자신에게 승인된 범위에서 진행한다.

## 3. 피드백 3절의 안전·인수 의미 수용

| 제공자 항목 | BXDL이 유지할 소비·판정 의미 | 관련 기존 인수 |
| --- | --- | --- |
| 3.1 명시 초기화·재개 | 신규 init, 기존 DB restart, 부분 init, DB 완료 후 BXDL 기록 저장 전 중단을 구분한다. 제품 기록 부재만으로 DB를 재초기화하지 않는다. existing restart의 누락/오타 거부는 **DB open·schema 등록 전** 엔진 경계에서 적용하며 BXDL 파일 존재 검사로 대체하지 않는다. init은 합의한 chain 자료에 따른 로컬 data 생성이며 새 네트워크/membership 생성이 아니다 | A2, CFG-01~03 |
| 3.2 cold 검사 | config/key 읽기·공개 identity 도출을 허용하되 full Spring 기동·DB open/recovery·listen/송신·서명·키 생성은 하지 않는다. DB 내부는 `NOT_CHECKED`, 일부 통과는 기동 준비 완료가 아니다. 실제 init/start에서도 canonical 검증·독점 접근을 다시 수행한다. 항목별/전체 결과·exit code·stdout/log 분리·비밀 제거의 실제 계약을 fixture로 받는다 | A3, PRE-01~02 |
| 3.3 역할별 상태 | process alive·초기화·sync·역할별 readiness·합의 진행을 분리한다. observer의 `running=false`나 `NOT_APPLICABLE`만으로 실패를 판정하지 않고 `IDLE`·`UNKNOWN`을 장애와 같게 취급하지 않는다. nullable/unknown·큰 정수·시각·비원자적 관측 의미는 제공된 DTO/fixture를 따른다 | A4, OBS-01~02 |
| 3.3 기동 실패·종료 보고 | HTTP 기동 전 실패와 HTTP 종료 후 결과를 해당 engine instance/시도에 연결하고 이전 실행의 stale 보고를 거부한다. 정상 종료는 필요한 작업 정리·저장소 close의 엔진 근거와 제품 process/service 관측을 조합해 판정한다. SIGTERM·process 소멸·launchd job 제거만으로 정상 close를 선언하지 않는다. 보고 유실·timeout·확인 불가는 결과 불명이며, 강제 종료에서 불가능한 성공 보고를 요구하지 않는다 | A4~5, LIFE-01~04, OBS-02 |
| 3.4 공급물·지원 조합 | source/dirty·JAR hash·console identity·manifest/fixture revision을 연결하고 BXDL은 exact JRE 공급자/patch/hash·package hash를 추가한다. 개발 후보 provenance와 정식 공급을 구분하며 Mac 최소 버전·native 조건을 실제 조합으로 인수한다 | A1·A5, ART-01~04, PLAT-01 |
| 3.5 확정 상태·재시작 | node hash/root는 **동일한 확정 블록 높이**에서 비교한다. 따로 얻은 latest 값을 원자적 snapshot으로 취급하지 않는다. 동일 DB/key/WAL 재시작은 원장·키·합의 안전 상태의 연속성을 뜻하며 정상 실행 중 바뀌는 DB/WAL의 물리 byte 불변을 요구하지 않는다 | A5~6, LIFE-01, QBFT-01~02 |
| 3.5 검증 경계 | source 회귀와 exact JAR/JRE/package 인수를 분리한다. 소비자 경로에서 Gradle 등으로 엔진을 재빌드하지 않는다. Mac 로컬 4-validator와 Linux multi-host를 별도 판정하고, P2P mTLS를 운영자/API 인증으로 해석하지 않는다 | A5~6, ART-01, QBFT-01~03, SEC-01 |

cold 무변경 검사의 전후 계측과 실제 실행·restart의 상태 연속성 검사는 목적이 다르다. 전자의 DB open/write 금지를 후자의 정상 WAL recovery 금지로 확대하지 않는다. 단절·불명 상태에서 자동 재초기화·key/WAL 삭제·강제 재시작·downgrade를 하지 않는 기존 원칙을 유지한다.

Mac의 첫 사용자 profile은 CLI와 engine가 같은 UID다. 논리 디렉터리 분리가 강한 보안 격리라는 주장은 하지 않는다. 기존 loopback/local control 경계를 유지하고 실제 launchd 종료·로그인·sleep/wake 관측은 BXDL 인수로 확인한다.

## 4. 완료 확인·공급 제출·인수 결과 교환

피드백 4·5절의 절차에 동의한다. 검토 회신, 양측 상세 계약 확정, NIGO 구현·회귀, G1-M 공급, BXDL 실제 인수, Linux/NIGO-05 후속 결과를 구분한다.

1. NIGO owner가 실제 호출법·schema·fixture revision과 정상·부정·중단 의미를 제안하면 BXDL owner가 adapter와 A1~A7의 해당 범위에서 소비 가능한지 회신한다. 이때 세부 계약·완료 기준의 합의 근거를 남긴다.
2. NIGO owner는 원문 7절 또는 연결된 결과 문서에 제공 범위, 구현 commit/PR, 계약·fixture, 실제 JAR/manifest/checksum 접근 위치, source/console identity, 검증 환경, 공식 재현 방법, 공급자 실행 결과·잔여 제한을 기록한다. 문서나 PR 병합만으로 공급 완료로 보지 않는다.
3. BXDL owner는 공급 revision/hash를 확인하고 engine lock을 고정한 뒤 product commit·package/JRE hash와 test-owned 환경을 기록한다. 누락·불일치는 인수 실패로 남긴다.
4. BXDL은 해당 A1~A6을 지정 Mac package로 실제 실행하고 source/Cargo/Gradle/npm/token/인터넷 없는 실행, 정상·부정·중단·재시작, 같은 확정 높이의 4-validator 결과를 검증한다. 공급자 근거와 소비자 결과를 각각 연결하고 실패·`NOT_CHECKED`·미실행을 보존한다.
5. 소비자 결과와 exact 조합·정제된 오류·잔여 owner를 공유 가능한 BXDL revision으로 회신한다. NIGO 원문 7·8절에 반영할 때도 해당 owner와 공유 절차를 따르며, 다른 세션의 작업 트리를 직접 변경하지 않는다.

`OPEN → ACCEPTED → IN_PROGRESS → DELIVERED → VERIFIED`의 기존 판정 주체와 의미를 유지한다. 후속 후보에는 실제 호출·field·fixture가 있지만 `PROPOSED`이며, 수신·정적 대조만으로 전체 인터페이스 합의나 제품 인수 완료를 선언하지 않는다. G1-M 공급/인수만 완료되면 그 범위를 명시하고 Linux/A7의 미완료를 가리지 않는다. REQ-0002 전체를 먼저 닫지 않으며 추후 범위 이관이 필요하면 양측 합의·원장 연결을 남긴다.

## 5. 현재 대기 항목과 다음 담당

| 항목 | 현재 및 다음 담당 |
| --- | --- |
| init/restart·cold 공식 호출 | 후보의 `engine-info`, `preflight`, `init`, `resume-init`, `run` 호출과 거부 조건을 수신. 첫 소비 범위는 identity 확인과 cold 검사이며 명시 init/restart·중단 복구의 제품 인수는 후속 |
| manifest·상태·startup/shutdown | 후보 manifest·runtime 계약·health fixture를 수신. BXDL은 실제 field·reason·실행 시도 연결 규칙을 소비하며, local readiness와 합의 진행을 구분하고 QBFT 종료 `UNKNOWN`을 보존 |
| 실제 공급 artifact와 결과 | development JAR/manifest 및 별도 공급자 결과 문서 제공됨. JAR은 7절의 dirty source 후보이며 clean 공급·선정 JRE와 제품 package·G1-M 인수는 남음 |
| JRE·지원 Mac 조합 | Java 21/arm64/RocksDB 후보만 정함. JRE 공급자·정확 patch·hash·재배포 자료, 최소 macOS·native/temp 조건은 BXDL이 선정하고 실제 G1-M으로 검증 |
| 격리 소비자 시험 환경 | test-owned 사용자 profile·경로·PKI·ports·4-validator 로컬 환경을 BXDL이 준비. 현재 사용자 node/data/key를 전용하지 않으며 실제 G1-M 환경 인수는 미실행 |
| Linux·NIGO-05 | Linux exact runtime/systemd/multi-host와 A7 유지보수는 해당 공급·환경이 준비될 때 별도 수행 |

추가 거절이나 새 계약명 제안은 없다. 제공된 호출·field의 실제 철자와 의미를 따르고 미제공 항목을 발명하지 않으며 엔진 안전 규칙을 BXDL의 파일 검사·DB 직접 조회로 대체하지 않는다.

## 6. BXDL의 이번 작업과 검증 경계

최초 회신 당시 공유 기반에는 Rust CLI, development package 조립·검증, 제품 JSON validation, metadata-only preflight가 있었고 다음 작업은 R1 setup이었다. 이후 R1과 후보 엔진 소비 작업의 최신 구현·테스트 결과는 [구현 상태](../docs/implementation-status.md)의 해당 revision과 연결된 evidence를 따른다.

이번 계약 원문 수신·정적 대조는 명시 init·engine start/stop·launchd 실행 또는 G1-M 설치·운용 인수 결과가 아니다. R1 입력 흐름이나 engine-info/cold 검사만으로 이 기능들의 완료를 표시하지 않는다. 제품 metadata preflight가 검사하지 않은 canonical/key/runtime/service 항목과 엔진 cold 검사가 검사하지 않은 DB/WAL·network·native runtime 항목은 해당 결과의 `NOT_CHECKED`를 유지한다.

최초 회신과 이번 계약 수신 기록의 근거는 문서·revision·원장 읽기 및 아래 artifact 정적 대조다. 이 기록 작업에서는 엔진 빌드·서비스 조작·키 생성·release·A1~A7 인수를 수행하지 않았고, NIGO 저장소에는 쓰지 않았다. 소비자 실행은 별도 결과에 실제 command·조합·범위를 기록하며, 과거 Go/Rust fixture 검증을 엔진 인수 결과로 재표기하지 않는다.

## 7. 2026-09-17 개발 후보 후속 수신

읽은 NIGO checkout은 PR #117 병합 HEAD `17c0bc3b63756915c18fa2942afdd73426bb2eac`다. 공급 JAR의 source/contract revision은 `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27`, `source.dirty=true`이며 두 revision을 동일시하지 않는다. `nigo-node-0.0.1-SNAPSHOT.jar`의 SHA-256은 `37070fbddaf1350b81952b9f3bd718b76f246557d7cd5abe2ec580a13d8de5fe`, 계약 fingerprint는 `0ac719e0175e9d5fb2e732e297b496e5725bd17f18d84bb53c6b51c235fd71cc`다. 계약 상태는 `PROPOSED`, 공급 channel은 `development`, `officialRelease=false`다.

제공자 계약 원문 4개와 `engine-manifest.json`의 정확한 bytes를 [수신 스냅샷과 provenance](../contracts/nigo/development-2026-09-17/README.md)에 보존했다. 최초 채취 당시 소스 계약과 JAR 내 4개 entry가 동일하고, 개별 hash·순서가 있는 aggregate fingerprint·JAR hash/size·embedded build metadata와 manifest의 대응 필드가 일치함을 읽기 전용으로 확인했다. 후속 재검사에서는 동일 HEAD의 source 계약 4개에 미커밋 변경을 관측했으며, 후보 JAR과 일치하는 스냅샷은 그대로 보존하고 차이를 README에 기록했다. 스냅샷의 존재나 `PROPOSED` 문구는 제품 코드가 임의 JAR을 승인할 근거가 아니다. 선택한 공급 bytes와 독립적으로 지정한 Java 실행 파일의 identity를 실제 소비 경계에서 고정·확인해야 한다.

첫 adapter가 소비할 정확한 호출은 `java -jar <JAR> engine-info`와 `java -jar <JAR> preflight --config=<absolute-node.json>`다. 전자는 envelope 없는 metadata이며 exit 0만으로 충분하지 않아 `buildInfoStatus=AVAILABLE`과 고정 identity를 확인한다. 후자의 정상 cold 결과는 exit 3의 `INCOMPLETE/RUNTIME_CHECKS_REQUIRED`다. 이를 기동 준비 완료나 제품 package 검증 실패와 혼동하지 않는다. native node JSON은 `chainFile`, `dataDirectory`, `backend`, `node` 구조이며 BXDL instance JSON과 다르다. mTLS/validator/peer 자료를 추측해 렌더링하지 않고 명시적인 native 입력을 소비한다.

제공자 task 문서에는 exact JAR 시험 8개 및 별도 source 회귀 결과가 있으나 manifest의 `evidence.supplierTests=NOT_RECORDED`와 `consumerAcceptance=NOT_RUN`은 원문 그대로 보존했다. 공급자 결과를 소비자 결과로 승격하지 않는다. QBFT 전체 worker 종료 확인의 `UNKNOWN`, follower 실패 전파·source failover·fresh sync 조합의 제한, clean 공급과 선정 JRE/native/launchd 및 로컬 4-validator 제품 인수는 계속 남아 있다. 이 절은 해당 개발 후보의 수신 기록이며 REQ-0002 전체 완료 판정이 아니다.
