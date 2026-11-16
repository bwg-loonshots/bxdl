# RC4-05 — 독립 관리 CLI와 고정 RC.4의 Mac 설치·서비스 인수

- 날짜: 2026-11-15
- 판정: **지정 Mac 단일 노드 설치·서비스 21사례 PASS. 독립 사후 감사 PASS.**
- 근거: [정제 JSON](./2026-11-15-rc4-macos-service.json) · [RC.4 도입 계획](../design/2026-11-15-rc4-adoption-plan.md) · [재현 도구](../tests/acceptance/README.md)

RC4-04 결과·상태 문서 9개를 `4ccc320`으로 커밋한 뒤, 이미 설치한 독립 관리 CLI B의 안정된 진입점으로 RC.4 runtime R을 새 관리 home·control·DB에 설치했다. 명시 초기화 1회, LaunchAgent 기동·정상 종료 각 2회와 같은 DB 재시작을 확인했다. 노드 패키지·내부 실행기 A·엔진 JAR·JRE·관리 CLI B를 재빌드하거나 교체하지 않았다. Rust 제품 코드와 NIGO 저장소는 변경하지 않았다.

서비스 시나리오는 1회, 자동 재시도는 0회이며 161.513초가 걸렸다. 직접 관리 CLI 호출은 35회다. 이는 위임된 자식 실행기나 모든 JVM 호출 수를 뜻하지 않는다. 환경은 macOS 26.6.2 arm64, 로그인된 사용자의 LaunchAgent이며 최대 동시 서비스 노드는 1개였다. 실행 시간은 이 시험의 관측값이며 성능 기준이 아니다.

## 고정한 조합

| 구성 | source / SHA-256 |
| --- | --- |
| 외부 관리 CLI B source | `65fe83f0d569fcf2231474342fa0d60ca72555d5` |
| B binary | `f08578809744d21313aad3c8cb39644107af67d5558661d2ecc9d81f2edb138c` |
| B signed CLI-only bundle | `0b03f7324c6b2b735aed9ab7793e65d151af1445f93e0506068289d19cb03a23` |
| 내부 실행기 A source | `d38027a474f1e8d4fb7b1d38ea17972ba88d93af` |
| A binary / worker | `d2d5c586bf2ab59ac3198cbcc0f89101fc18d31e6f4ec7812b428e481a63f647` |
| RC.4 runtime R archive | `a9f67988e785260b35504bd79a19df4a4820e72eafbf537328ca2e31f7e16f60` |
| R manifest | `fe21e898f5ace8be57ad838620f14da1c3e07b3ffcfbcb969fd63a67854a142e` |
| NIGO RC.4 source | `037c98c315d5322fa7da266ca0254d61f3863622` |
| NIGO JAR | `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` |
| Temurin Java launcher | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |

R은 [RC4-03](./2026-11-15-rc4-runtime-assembly.md), B는 [RC4-04](./2026-11-15-rc4-cli-harness.md)에서 고정한 시험용 서명 개발 배포본이다. profile은 `BXDL_RC4_EXECUTOR_V1`, JRE는 Temurin21.0.12.1+1-LTS다. CLI root와 노드 관리 home을 분리했으며, 모든 관리 명령은 `cli install`이 만든 진입점을 사용했다. 설치 payload 269파일·326,373,420bytes, 그중 JRE259파일의 membership·hash·size·mode를 설치 직후와 종료 후 전수 대조했다.

## 설치 문답·초기화·운용 결과

RC4-02 fresh fixture에서 RC4-04 준비기가 만든 수동 서비스 입력을 읽어 새 시험 폴더로 복사했다. 입력9파일에는 credential6파일이 포함되고 공급물은 archive·공개키·lock 3파일이다. 기존 RC.3 control·DB를 사용하거나 시험 자격증명을 새로 발급하지 않았다. 원래 4-validator chain 입력 bytes는 보존하고, 이 서비스 시험의 native 설정은 peer 없는 수동 시작으로 사용했다.

| 과업 | 실제 관측 |
| --- | --- |
| `setup new local --from …` → archive 입력 후 `:cancel` | 저장한 archive 유지, revision0·DRAFT·NOT_PREPARED, 다음 입력은 `trustPolicy` |
| `setup resume local` | archive를 다시 묻지 않고 신뢰 정책부터 계속해 revision1 준비 |
| `setup edit local --field instanceId` → `setup resume local` | 편집 직후 검토 무효화, 기존 생성 파일 보존, 승인 후 revision2; 제품의 instanceId만 변경되고 native 내용 동일 |
| 미완성·준비·수정·재준비·설치·정지 상태 `setup list/show` | 7쌍 모두 성공, 각 전후 관리 home의 bytes·inode·권한·mtime/ctime 등 snapshot 동일 |
| `setup apply local` 거절 → 승인 | 거절 시 쓰기 없음; 승인 시 package 설치·cold 검사·등록·이름 연결, DB는 아직 없음 |
| 이름 기반 명시 `init` | 새 DB 생성과 INITIALIZED 확인 |
| `start` → `status --wait` → `logs`/`diagnose` → `stop` | 로컬 READY·완전수집·정상 종료 |
| 같은 DB에서 다시 `start/status/stop` | 새 attempt, 같은 data device/inode와 engine-instance identity, 정상 종료 |
| 정지 상태의 `setup apply` 재호출 | package·control·DB 전후 전체 snapshot 불변 |

준비는 제품 설정을 `--from`으로 가져온 순차 문답이다. 빈 초안의 14개 제품 필드 전체 입력, EOF/HUP, 실제 사용자의 이해도·편의성을 이번에 검증한 것으로 확대하지 않는다. 초기화와 두 번의 run report에서 lock이 요구한 전체 engine identity가 일치했다. 두 번의 READY 이유는 모두 `MANUAL_START_INITIALIZED`다. 글로벌 합의는 **NOT_CHECKED**이며 거래·확정이나 RPC 검사를 수행하지 않았다.

### 설치 원자료 경로가 없어도 운용되는지

첫 apply의 V2 binding이 archive·공개키·lock을 `control/trust`의 관리 사본으로 참조하고 세 사본이0600·동일 hash인지 확인했다. 이어 **이번 시험이 복사해 설치에 사용한 supply3파일만** 별도 위치로 옮겼다. 원본 RC4-04 fixture나 고정 R 파일을 옮기지 않았다. 제품/native·chain·credential 입력 경로는 계속 사용할 수 있게 유지했다.

해당 공급 경로가 없는 상태에서 적용 결과 조회, 명시 init, 두 번의 서비스 기동·정상 종료, 로그·진단, 완료 재apply와 정지 상태 조회가 성공했다. 이후 세 파일을 복원하고 hash·파일 identity를 확인했다. 원본과 복사 입력도 그대로 유지됐다. 이 결과는 설치 이후 관리 사본 재사용 범위이며 등록 전 공급물 부재나 모든 운용 입력 제거를 허용한다는 뜻이 아니다.

### 초기화 보고서와 진단

호출자 umask를 `022`로 기록·설정하고 `--expect-complete-diagnostics`를 필수로 적용했다. 고정 B→A→JVM 조합에서 새 init report는 **0600**으로 생성됐다. 생성 직후·진단 전후·두 서비스 생애주기 종료 후의 hash·inode·권한·metadata가 같았다. 보고서를 사후 chmod하거나 다시 초기화하지 않았다.

- `logs`: `SUCCEEDED / LOGS_COLLECTED`, `partial=false`
- `diagnose`: `SUCCEEDED / DIAGNOSTIC_WRITTEN`, `partial=false`
- 양쪽 모두 `initialization.report = COLLECTED / PRIVATE_SNAPSHOT`, `init / INITIALIZED` 이벤트 포함
- report SHA-256: `ab7967647bcd54d9b0e2446c3172144d72d17157841b5bc4addcbc623f3c93be`

원문 stdout/stderr를 정책상 제외하는 항목은 완전수집 실패와 구분한다. 기존 RC.3의0644 보고서는 변경하지 않았으며 해당 과거 partial 기록도 그대로다.

## 불변성·정상 종료와 독립 감사

worker2개는 모두 A와 같은 bytes였다. R archive·설치 payload·JRE·B 설치 선택과 실행 파일, 원본 fixture 입력의 hash가 유지됐다. 같은 DB 보존은 재시작 전후 data 디렉터리와 engine-instance identity의 보존을 뜻한다. 운용 중 DB/WAL 전체 bytes가 고정됐다고 주장하지 않는다. 완료 재apply 시에는 정지된 DB의 전체 snapshot도 비교했다.

| 종료 관측 | 결과 |
| --- | --- |
| 정상 stop | 2회 |
| runner가 확인한 CLI/엔진 PID | 38개 모두 부재 |
| 독립 감사 PID | runner PID를 포함한39개 모두 부재 |
| LaunchAgent | 2개 모두 NOT_LOADED |
| HTTP/P2P 시험 TCP LISTEN | 2포트 모두 없음 |
| 시험 root의 남은 제품 프로세스 | 0 |
| 기존 GUI/user/system 관련 job·설치 plist 기준선 | 동일 |

독립 감사는 frozen 하네스6파일·현재 저장소 코드, 전체 payload와 worker, init/run report identity, 남은 파일 및 OS 상태를 읽기 전용으로 재대조했다. query7쌍과 재apply3root의 저장된 전후 snapshot도 검증하고 최종 snapshot을 현재 파일과 대조했다. 당시 snapshot 캡처 시점과 공급 부재 기간은 runner가 기록한 근거이며 독립 검토자가 실시간으로 다시 관찰한 결과는 아니다. credential 본문은 재열람하지 않고 runner의 hash 검사와 현재 권한을 근거로 삼았다. syscall/process-tree trace를 수집한 것은 아니다.

첫 사후 감사는 샌드박스에서 `ps` 실행이 거절돼 `INCOMPLETE / PermissionError`로 남았다. 원인 위치를 읽기 관측으로 확인하고 OS 조회 권한을 적용한 별도 감사에서 PASS했다. 첫 감사 파일도 보존했다. **서비스 시나리오·CLI·JVM을 재실행하지 않았으며 원 서비스 PASS 결과를 수정하지 않았다.**

## 도구 보완·자동 검사·근거 보존

기존 서비스 runner에 명시 옵션 `--exercise-setup-recovery`를 추가했다. 취소/재개/편집, 관리 공급 사본의 확인과 일시 경로 부재, 충돌 시 덮어쓰지 않는 복원, query·재apply snapshot 보존을 다룬다. 기본 RC.3 시나리오는 유지한다. 실패 시 원래 결과를 보존하고 자동 서비스 재시도나 강제 kill을 하지 않는 정책도 유지한다. 옵션·실행 예시는 [인수 도구 문서](../tests/acceptance/README.md)를 따른다.

실행 하네스는 `4ccc320`을 기반으로 한 미커밋 보완본이며 제품 B의 빌드 source와 구분한다. 6파일의 SHA-256을 고정한 사본으로 실행했고 실행 전후 사본·저장소 코드가 같았다. 자동 검사는 코드136파일의 hash가 전후 같음을 확인했다. 정확한 full base와 파일별 pin은 JSON에 기록한다.

| 검사 | 결과 |
| --- | --- |
| `make check` | PASS,446.930초 |
| Rust | 596 PASS |
| Python discovery | 165개 중163 PASS·기존 opt-in2 SKIP |
| 서비스 도구 오프라인 회귀 | 27 PASS, 위 Python 수에 포함 |
| fmt / Clippy | PASS / PASS |
| Linux 타입 검사 | 이번 재실행 없음; Rust 변경 없으며 RC4-04의 PASS는 이력으로 보존 |
| 실제 Mac 서비스 / 독립 사후 감사 | 21사례 PASS / PASS |

원 stdout·PTY·snapshot·진단·생애주기 결과·검사 로그·사후 감사는 private 시험 폴더에 보존했다. 선택된 근거90파일의 inventory SHA-256은 `1002779a21d8a51e64f0f4d09212f1134479c8be7a0e1f18e9fb4dd60279062e`다. 원 실행 결과 SHA-256은 `5fd6830a78553e6e1e2e59c9740a1f38a6e039a2b2d0d029bda1e03fb0315bdb`, 최종 독립 감사는 `c0bc946bab39871c6156f9d27cf00964728e2dc798b0f036349cbc0a1616ea62`다. 공개 JSON에 주요 근거의 digest·크기를 연결했다. 원 자격증명·signed transaction·DB/WAL·archive·개인 경로·PID 목록은 Git에 넣지 않는다.

## 다음 범위

**RC4-05는 DONE이며 다음은 RC4-06**이다. 새 동종 4-validator 망의 확정·catch-up·무정족수·두 복구·wrong-pin, 선정 RPC21사례/52read, RC.4를 지원하는 두 관리 CLI 사이의 실행 중 업데이트/복귀를 인수한다. 현재 B와 같은 RC.4 profile을 지원하는 두 번째 관리 CLI의 실제 clean source identity를 먼저 준비해야 한다. R/A/JAR/JRE는 유지한다. 새 NIGO 구현 선행 요청은 확인되지 않았다.

네트워크·RPC·실행 중 CLI 전환, 빈 초안 전체 문답·EOF/HUP, 실제 사람 과업(CLI05-04), 로그아웃·sleep/wake·crash, 기존 DB migration·혼합망·안전 제거·A7·Linux/systemd·Docker·LaunchDaemon·장기/성능·J5는 이번 결과 밖이다. development·officialRelease=false·계약 PROPOSED·REQ-0002/0003 OPEN·고객 납품 NOT_GRANTED를 유지한다. RC4-04 당시 제품 서비스 NOT_RUN과 RC.3 과거 결과는 각 시점의 기록으로 보존한다.
