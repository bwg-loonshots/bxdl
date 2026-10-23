# 초기화 보고서 생성 권한 — 원인 재현과 독립 CLI 개선

- 날짜: 2026-10-23
- 판정: **CLI05-03-DIAG-01의 새 초기화 보고서 생성·진단 수집 개선 PASS. 기존 0644 보고서는 자동 복구하지 않는다.**
- [정제 JSON](./2026-10-23-private-initialization-report.json) · [설계](../design/2026-10-23-private-initialization-report.md) · [실행 도구](../tests/acceptance/README.md#cli05-03-독립-관리-cli와-기존-rc3-조합)

[CLI05-03 당시](./2026-10-23-cli05-runtime-acceptance.md) 성공한 초기화의 보고서가 0644여서 `logs/diagnose`에 수집되지 않았다. 당시 umask는 측정하지 않았으므로 과거 결과는 그대로 보존한다. 이번에는 기존 CLI와 개선 CLI를 명시한 caller umask로 각각 실행해 원인을 대조했다.

## 원인과 변경

NIGO RC.3 `EngineProcessReport`는 `FileChannel.open(CREATE_NEW, WRITE)`로 새 파일을 만들며 POSIX mode를 따로 지정하지 않는다. 독립 관리 CLI → 고정 실행기 → 일회성 초기화 JVM 경로가 호출자의 umask를 상속했다. 기존 CLI의 caller 022에서 보고서 0644, 077에서 0600을 확인했다. 서비스는 기존 LaunchAgent의 `Umask=63`(077) 때문에 두 경우 모두 보고서 0600이었다.

초기화 성공 판정용 reader와 달리 진단 reader는 정확한 0600을 요구한다. 따라서 초기화 성공과 진단 부분 완료가 함께 발생할 수 있었다. 원인과 관련된 RC.3 엔진 소스는 현재 NIGO 소스에서도 같았다.

독립 BXDL 바이너리의 `main()` 시작에서 기존 `rustix`의 안전한 API로 umask 077을 한 번 설정했다. 스레드·자식 실행 전이며 고정 실행기와 초기화 JVM이 이를 상속한다. 호출자 셸, 기존 파일, 라이브러리를 직접 사용하는 프로세스의 mask는 변경하지 않는다. 기존 RC.3 package 내부의 구형 CLI를 직접 실행하면 그 바이너리의 예전 정책이 적용된다.

진단 reader의 소유자·정확한 0600·단일 link·안전한 부모 경로 검사는 유지한다. 보고서를 미리 생성하거나 사후 chmod하지 않는다. NIGO JAR, runtime package, 고정 실행기, JRE를 재빌드·교체하지 않았다. 새 노드 package archive의 외부 파일도 이 기본 정책에 따라 0600이 되며 archive 내부 bytes와 설치 payload의 명시 mode는 유지한다.

## 실제 비교 — 네 조건, 각 서비스 16사례

macOS 26.6.2 arm64의 로그인된 사용자 LaunchAgent 환경에서 각 조건을 새로운 private control·DB·포트로 실행했다. 기존 단일 validator 시험의 공급물과 운용 입력만 재사용했다. 관리 명령은 `cli install`이 소유한 안정 진입점으로 실행했다.

| 관리 CLI | caller umask | 새 init report | `logs` / `diagnose` | init source |
| --- | --- | --- | --- | --- |
| 기존 B | 022 | 0644 | INCOMPLETE / INCOMPLETE | UNAVAILABLE |
| 기존 B | 077 | 0600 | SUCCEEDED / SUCCEEDED | COLLECTED |
| 개선 개발본 | 022 | 0600 | SUCCEEDED / SUCCEEDED | COLLECTED |
| 개선 개발본 | 002 | 0600 | SUCCEEDED / SUCCEEDED | COLLECTED |

첫 조건은 의도한 원인 재현 PASS이며 완전수집 성공이 아니다. 실제 코드는 `LOGS_PARTIAL`·`DIAGNOSTIC_PARTIAL`, init source 사유는 `MISSING_UNSAFE_OR_CHANGING_INPUT`이었다. 나머지 조건은 `LOGS_COLLECTED`·`DIAGNOSTIC_WRITTEN`, `partial=false`, init source `PRIVATE_SNAPSHOT`과 실제 `init/INITIALIZED` 이벤트까지 확인했다. raw stdout/stderr의 정책상 제외는 허용하며 원문 수집 성공으로 해석하지 않는다.

각 16사례는 설치된 진입점 확인, 순차 준비, 조회 무쓰기, apply 거절·승인, 명시 init, 이름 기반 start/status/logs/diagnose/stop, 같은 DB 재시작, 정지 상태 reapply·조회, 원 자료와 runtime 보존, 종료 정리를 포함한다. 자동 합의를 시작하지 않는 peerless 설정으로 READY는 `MANUAL_START_INITIALIZED`이며 글로벌 합의는 `NOT_CHECKED`다.

네 조건 모두 init report의 생성 직후·진단 전후·전체 수명주기 종료 후 mode·device/inode·SHA-256·size·mtime/ctime가 같았다. 각 설치본의 payload 270파일·325,299,323bytes, JRE 259파일과 worker 2개를 고정 원본과 대조했다. 원본 및 복사한 운용 입력도 각 12파일씩 불변이었다.

정상 stop 8회 뒤 LaunchAgent job 8개 NOT_LOADED, 기록된 CLI/Java PID 합집합 108개 부재, 시험 8포트 LISTEN 0, 시험 root의 잔존 프로세스 0을 별도 읽기 전용 감사로 재확인했다. 강제 kill, DB/WAL 삭제, 재초기화, 엔진 교체는 수행하지 않았다.

## 기존 0644 보고서의 읽기 전용 재조회

개선 CLI로 기존 B·022 조건의 정지된 control에 `logs --instance`와 `diagnose --instance --output <새 경로>`를 실행했다. 둘 다 exit 5, INCOMPLETE와 init source UNAVAILABLE을 유지했다. 기존 report 0644와 전체 control 70항목·data 30항목의 bytes·membership·mode·inode·mtime/ctime가 호출 전후 같았다. 새 진단 export만 0600으로 생성됐으며 init/start는 호출하지 않았다.

독립 감사는 실제 응답·현재 원 report·원 결과·실행 스크립트를 대조했다. 전체 control/DB의 호출 전 snapshot은 실행 중 메모리에서 비교했고 별도 파일로 보존하지 않았으므로, 독립 감사가 그 과거 snapshot을 재구성한 것은 아니다. 기존 0644를 자동으로 고치거나 과거 자료가 완전 수집됐다고 표시하지 않는다. 명시적인 과거 보고서 복구 도구는 별도 후속 범위다.

## 빌드와 고정 공급물

| 구분 | identity |
| --- | --- |
| 기존 관리 CLI B revision | `3a0bb5caa08096454fcb6c88b3186e9a3be02a19` |
| 기존 B 바이너리 SHA-256 | `99bb09583a9615548d23eb5d088027ccc2831ba5cc0c0f3dacac388209bf2fb8` |
| 개선 개발본 base revision | `d0e6d0da39a9ae08b9a0475969dedf92a7eea3c6` |
| 개선 바이너리 SHA-256 | `226949d83d86fb77c31a4317523c89d12f781b451161598aea4167f130d8a81f` |
| 개선 관리 CLI archive SHA-256 | `ee4eb6662c99fdde15489ece28b183dffd8f0c418792c7345647ad7ef57674ba` |
| 빌드 입력 inventory SHA-256 | `db26981258b93daadda506d119f46c6761e765c9f412b014967c2364415ee1a3` |
| 기존 RC.3 archive SHA-256 | `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017` |
| 고정 실행기 SHA-256 | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| NIGO RC.3 JAR SHA-256 | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| Temurin Java SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |

개선본은 미커밋 소스로 `tools/build-management-cli.py --development --allow-unsigned-development`로 만든 **unsigned development** 배포본이며 revision은 `development`다. 정식 서명 release로 표시하지 않는다. 빌드 당시 208개 파일의 per-file hash를 보존하고 바이너리·archive hash로 실제 실행본을 식별했다. 빌드 이후 갱신한 결과·가이드 문서는 시험 바이너리에 다시 묶지 않았다.

지원 profile `BXDL_RC3_EXECUTOR_V1`, NIGO source `7ec5abd8c98e1cf070763a16f815e9281b74a5c1`, Temurin21.0.12.1+1-LTS full JRE 49모듈은 그대로다. 공개키·engine lock·manifest pin, 실행 하네스·원 결과·감사·검사 로그의 hash는 JSON에 기록했다. 원 stdout·PTY·보고서·DB·키·archive와 개인 경로/PID 목록은 Git에 넣지 않았다.

## 자동 검사와 남은 범위

| 검사 | 결과 |
| --- | --- |
| `make check` | fmt·Clippy PASS, Rust592 PASS; Python99개 중97 PASS/2 opt-in SKIP |
| `cargo check --locked --target x86_64-unknown-linux-gnu` | PASS, Linux 타입 검사이며 실행 인수는 아님 |
| 실제 Mac 서비스 비교 | 4조건 × 16사례 PASS; 기존022 partial 재현, 나머지0600 완전수집 |
| 수정 CLI의 기존0644 재조회 | 원본 무변경·partial 보존 PASS |
| 읽기 전용 독립 사후 감사 | 4조건 및 legacy 재조회 PASS |

추가 Rust 회귀 2개는 실제 CLI subprocess를 caller 002/022/077에서 실행해 초기화 자식의 상속 mask, 새 report 0600, 부모 셸 mask 불변, 실패 시 UNKNOWN·재시도 없음, archive 내부 bytes와 설치 mode 보존을 검사한다. stub이므로 NIGO 초기화 성공 증거는 위 실물 시험과 구분한다. 서비스 하네스의 오프라인 회귀 5개를 더해 엄격한 수집 판정과 mask 관측·복원, 기존 report 무수정을 검사했다.

실제 JVM 내부 umask를 별도로 계측하지는 않았다. caller 조건과 새 report mode, 코드의 상속 경로, subprocess 회귀를 함께 대조한 결론이다. 독립 감사는 같은 실행의 증거와 종료 상태를 재대조했으며 별도의 재실행으로 세지 않는다.

이번 결과는 새 `init` 경로를 검증한다. `resume-init`도 같은 진입 정책을 상속하지만 실제 복구 시나리오는 재실행하지 않았다. 4-validator 네트워크·실행 중 CLI 전환은 이번 변경 뒤 재실행하지 않았으며 이전 CLI05-03 결과와 구분한다. CLI05-04의 작성자가 아닌 사람의 과업은 **NOT_RUN**이다. 로그아웃/sleep·crash/power-cut·기존 runtime/DB upgrade·Linux 실운영/systemd·Docker·LaunchDaemon·장기/성능·J5·고객 정식 배포도 이 결과로 완료 처리하지 않는다.
