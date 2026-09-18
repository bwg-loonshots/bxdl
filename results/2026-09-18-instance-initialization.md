# 인스턴스 등록·명시 초기화 검증 — 2026-09-18

BXDL `d55b584b48a4fa869526d5cbe94609c4913a92fa` 이후 구현을 macOS arm64에서 검증했다. 등록된 개발 package로 `instance register/show`, `preflight --instance`, `init`, `resume-init`을 호출한다. launchd·네트워크 노드·4-validator·운영 DB 인수 결과가 아니다.

## 코드 검사

| 검사 | 결과 |
| --- | --- |
| `make check` | fmt·Clippy `-D warnings` 통과, 테스트 **162개 PASS** |
| `make build` | host release CLI 생성 |
| `make build-macos` | aarch64-apple-darwin release CLI 생성 |
| `make check-linux` | x86_64-unknown-linux-gnu 타입 검사 통과. Linux 실행/서비스 검증 아님 |
| 원격 CI | 이번 구현 턴에서는 실행하지 않음 |

테스트 구성은 library 141, CLI 9, CLI 통합 2, setup CLI 5, setup 안전성 5개다. 신규 검사는 설치된 JRE 전체 변조/누락/추가·receipt 불일치, private control/상속 lock·교체 탐지, 초기화 상태·입력 pin·재개 조건, strict stdout/JSONL/engine journal 결합, timeout·출력 한도·살아 있는 child 보존과 회수를 포함한다.

기존 FIFO 안전성 테스트는 기본 sandbox의 `mkfifo` 제한에서 막혔다. 테스트 전용 임시 경로에 필요한 권한으로 전체 `make check`를 다시 실행해 모두 통과했다. 테스트를 건너뛰거나 FIFO 정책을 완화하지 않았다.

## 시험 package와 runtime

공유 NIGO checkout과 요구사항 원장은 수정하지 않았다. 최근 NIGO 소스 변경을 섞지 않고 기존 공급 후보 source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false, PROPOSED/development/officialRelease=false를 사용했다. NIGO 소스 빌드 없이 공급된 JAR로 실행했다.

| 대상 | SHA-256 |
| --- | --- |
| NIGO JAR | `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455` |
| 최종 Mac CLI 및 package 안 CLI | `89fa73d536a3c1943e4fe4fc08e105e54bdb688db4598d6de91732a9da860e18` |
| host CLI | `44371506591f23a9ddd00f71c7dc5db9f2d851ecf04ed5ada82d3c279408978a` |
| 최종 시험 archive | `a086739988ce7a17c968613609c937e4f1cb74f9ad1d10b10969ae649899be61` |
| manifest | `c70f322dc2a1f7f6b25aa6119520bd73e3935b66bd5fc2bdd95c60397aa9d62a` |
| Java launcher | `94b66f2cc8edfba9e0e8b25abe93e4c3617d2cc1af7e8eaec0f5b2afe0df5c12` |
| `runtime/lib/modules` | `cc0bbb39690b3c78867b9fad0847442ea7a6a98bff16942a79cb9ccdd4d7649d` |

최종 archive는 명시 unsigned-development 시험 자료이며 payload 141개, 248,726,022 bytes다. 같은 archive를 새 폴더에 설치한 뒤 그 안의 Mac CLI/JRE/JAR로 시험했다. 이 archive·JRE·시험 키·DB·원문 실행 자료는 저장소에 넣지 않았다.

첫 초기화에서는 앞 단계의 cold 전용 jlink runtime에 `jdk.management`가 없어 `RuntimeMonitorReader`의 `com.sun.management.OperatingSystemMXBean` 로딩이 실패했다. 엔진 exit 74와 FAILED report를 BXDL이 UNKNOWN으로 보존했으며 기존 초기화 흔적을 지우거나 덮어쓰지 않았다. 별도 시험 JRE를 `java.se,jdk.crypto.ec,jdk.unsupported,jdk.management`로 생성하고 새 archive/설치본에서 재검증했다. Java launcher bytes는 같지만 전체 runtime inventory는 달랐다.

runtime은 로컬 Oracle 21.0.7에서 만든 시험용 jlink 결과다. 정식 공급자·patch·최소 OS·필요 모듈 전체·완전한 SBOM/NOTICE·재배포 승인은 아직 선정하지 않았다. cold 성공을 native/runtime 준비 완료로 소급하지 않는다.

## 실제 JAR 실행

새 시험 전용 데이터 경로와 private config를 사용했다. 이전 cold fixture의 공개 chain과 시험용 NGVK/TLS 자료를 읽기 전용으로 참조했다. 구성은 한 validator, 4개 membership identity, peer 없음, sync/auto-start 비활성이다. 사용자 서비스·키·DB는 사용하지 않았다.

| 시험 | 관측 결과 |
| --- | --- |
| 새 instance 등록 | exit 0, NOT_STARTED; 데이터 디렉터리 미생성 |
| 등록된 instance show | exit 0; 저장 상태 조회 |
| 등록된 preflight | exit 5, configurationBinding=MATCHED; runtime 검사는 NOT_CHECKED |
| 명시 init | exit 0, INITIALIZED; 실제 RocksDB 열기·genesis 생성·닫기, stdout/report/NIGO journal/owned PID 일치 |
| 완료 조회 | INITIALIZED, serviceRegistration=NOT_REGISTERED, runtimeReadiness=NOT_CHECKED |
| 완료된 instance의 init/resume 반복 | exit 3, INSTANCE_INITIALIZATION_STATE; 재초기화하지 않음 |
| 초기화 후 cold preflight | exit 5; DB 파일 내용 hash 전체 불변 |

초기화 genesis는 `0x65c31200d7990a224c1ec5c33d69c7c8465747b268ee2972dbb1ca3eb783c83a`이며 아래 재개 시험도 같은 genesis를 확인했다. 이 성공은 단발 저장소 초기화·종료에 관한 증거다. 지속 node run·WAL 장애복구·quorum·peer handshake·서비스 수명주기 준비 완료를 뜻하지 않는다.

## 실제 CLI crash와 재개 fixture

최종 package와 같은 Mac CLI에서 두 사례를 분리해 실행했다.

1. **실제 controller crash:** 이번 시험에서 실행한 Java의 PID·부모·시작 식별자·operation 경로를 대조한 뒤 해당 Java만 SIGSTOP하고 BXDL controller만 SIGKILL했다. `instance show`는 UNKNOWN/operationBusy=true, `resume-init`은 INSTANCE_BUSY였다. 같은 Java를 SIGCONT한 뒤 정상 종료와 lock 해제를 관측했다. 엔진 journal은 INITIALIZED였지만 BXDL은 UNKNOWN을 유지했고 resume은 INSTANCE_RESUME_NOT_ELIGIBLE로 거부했다. 잃어버린 stdout/종료 증거를 자동 채택하지 않았다.
2. **합성 중단 지점의 실제 resume:** 별도 성공 시험 DB를 복사하고 시험 harness가 NIGO INITIALIZING 및 BXDL UNKNOWN checkpoint를 준비했다. 실제 JAR의 `resume-init`은 exit 0/INITIALIZED였고 기존 genesis가 일치했다. 이는 **합성한 checkpoint 경계** 시험이며 자연 발생 crash 순간을 포착해 재개한 결과로 주장하지 않는다. 제품은 이 journal 조작 기능을 제공하지 않는다.

복구 시험은 PATH를 비우고 빈 cwd에서 수행했다. source fixture·package의 입력 154개가 전후 동일했고 노드 run/서비스는 호출하지 않았다. 최종 private recovery evidence SHA-256은 `31cb558fa1c8952905447e2bc0c81b2e952aeb4dceb3948fe584d7af1cbbad95`다. 중간 harness의 조기 종료 판정 실패는 최종 PASS 근거에 포함하지 않았다.

## 남은 인수

launchd start/status/stop과 실제 health/종료 증명, 동일 데이터 재시작, G1-M 4-validator, setup의 package 선택·등록 연결, runtime 정식 공급 선정이 남았다. UNKNOWN의 수동 채택·이동/복원·key/config migration·attempt 보존/GC도 아직 없다. Linux/systemd와 Docker는 후속 gate다. [사용 가이드](../docs/instance.md)와 [구현 설계](../design/2026-09-18-instance-initialization.md)의 범위를 따른다.
