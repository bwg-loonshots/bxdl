# UX-02 콘솔 홈·카탈로그 검증 — 2026-10-22

## 범위와 기준

- UX-01과 터미널 체험판 커밋 `cbf5c37` 이후의 UX-02 소스 구현이다.
- 실제 `bxdl console [--home <root>] [--instance <control>]` 진입점, 홈·노드/세션 목록·명시 기존 경로 읽기, 승인한 목록 저장/갱신/숨기기를 연결했다.
- [현재 가이드](../docs/console.md)와 [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)을 따른다. Python 터미널 체험판의 가상 설치/운용을 실제 구현으로 합산하지 않는다.
- NIGO source/JAR/JRE/lock·기존 accepted archive를 변경하지 않았다. 새 console을 기존 설치 CLI에 덮어쓰지 않았다.

## 실행한 검사

| 검사 | 결과 | 범위 |
| --- | --- | --- |
| 최종 `make check` | PASS | fmt, Clippy `-D warnings`, Rust 427개, Python 50개 |
| Linux 타입 검사 | PASS | `cargo check --locked --target x86_64-unknown-linux-gnu`. Linux 실행/systemd 인수와 다름 |
| console CLI 통합 17개 | PASS | 아래 UX-02 회귀, 전체 Rust 합계에 포함 |
| 실제 Mac PTY smoke | PASS | 별도 실제 터미널 입력과 사후 파일 비교, 아래 범위 |
| 문서·diff 검사 | PASS | 로컬 링크, 코드 블록, `git diff --check` |

Rust 합계는 unit 358 + 기존 CLI 16 + CLI integration 2 + console 17 + setup 7 + install 14 + network 8 + safety 5다. 기존 UX-01 대비 44개 증가했으며 별도 focused 검사 횟수를 중복 합산하지 않는다. 최초 전체 426개 통과 뒤 console의 입력 I/O 장애를 공통 exit 7에 맞추고 무쓰기 회귀를 추가했다. 위 합계는 그 보완 후 다시 실행한 최종 결과다.

## 확인한 경계

- 홈·목록 조회, 도움말, 비TTY/JSON/잘못된 인자 거부, 승인 Enter·취소·EOF에서 관리 폴더를 생성하지 않는다. 비TTY·잘못된 옵션은 입력도 소비하지 않는다. I/O 장애는 exit 7이며 원문 오류 canary를 내보내지 않는다.
- 명시 선택한 등록 V1/V2, 설치 workflow, 제품 draft, network 세션의 지원 형식·root identity를 검사한다. 고정된 공개 단계만 투영하고 runtime READY·합의 상태로 승격하지 않는다.
- CLI 일치는 설치 `bin/bxdl`과 현재 실행 파일의 정확한 bytes 비교다. 불일치/비교 불가와 실행 승인을 구분하며 해당 파일을 실행하지 않는다.
- default 화면·목록은 private target/credential 경로와 engine 원문 reason을 노출하지 않는다. 사용자 요청인 명시 상세에서만 control/session 위치와 인용 처리한 기존 명령을 보여준다.
- 카탈로그 root/marker/lock/checkpoint의 private 권한·소유권·no-follow·identity를 검사하고, 불완전/손상된 기존 root를 채택·chmod·초기화하지 않는다. 크기·항목·revision 상한과 중복 ID/target·알 수 없는 schema를 거절한다.
- 저장 잠금은 짧게 유지하며 다른 작성자가 갱신한 snapshot의 저장을 거절한다. 같은 별칭은 독립 ID로 구분하고 재조회만으로 목록을 쓰지 않는다. 갱신은 별도 승인 뒤 ID/별칭을 유지한다.
- 신규 관리 위치가 target 자체나 DB/package/키·제품/native/계획 출력 위치를 침범하면 생성 전에 거절한다. 기존 marker가 유효한 home의 지정 `instances/<항목>`·`sessions/<항목>`과 다른 운용 namespace의 형제 관계는 허용한다.
- V2 원본 공급 자료 부재 및 이후 원본 경로의 symlink 교체가 현재 managed 입력의 읽기·경로 검사를 막지 않는다. 기존 V1/V2/workflow schema는 변경하지 않는다.
- target 교체·누락 때 자동 연결하지 않고, 목록 숨기기는 원본 폴더를 열거나 삭제하지 않는다. catalog 저장 실패에서도 기존 draft를 보존하고 기존 API로 열 수 있는 경로를 유지한다. 별도 등록 reader fixture는 잠금 중 UNKNOWN과 journal을 변경하지 않음을 확인했다.

## 실제 터미널 확인

현재 debug CLI를 Mac의 격리된 임시 폴더에서 실행했다. 첫 홈에서 도움말과 빈 두 목록을 열어 무쓰기를 확인한 뒤, 예제 제품 설정으로 만든 테스트 소유 draft를 명시 연결했다.

목록 저장 확인에서 도움말과 Enter로 거절하면 home이 없었다. 이후 별칭과 `y`를 입력해 저장하고 0700/0600을 사후 확인했다. 새 콘솔에서 세션 목록을 열고 저장 기록 재조회·명시 숨기기를 수행했다. 원래 draft의 모든 파일 bytes·inode·mode는 전후 동일했다.

같은 draft의 데이터 위치 안에 새 관리 root를 지정한 경우에는 `CONSOLE_TARGET_WRITE_CONFLICT`로 거절했으며 data 폴더도 생성되지 않았다. 실제 터미널 EOF는 exit 5였고 이미 저장된 catalog는 보존됐다. 시험 폴더는 정리했다. 엔진·launchd·HTTP·실제 DB 실행은 이 PTY 시험에서 하지 않았다.

## 판정과 후속

UX-02 소스 구현·로컬 회귀와 실제 TTY의 읽기/목록 변경 확인을 완료했다. UX-03은 준비자료·제품/network 설정을 하나의 흐름으로 연결한다. 설치 실행·실제 재개는 UX-04, runtime 상태·반복 운용은 UX-05, 지원 보고서는 UX-06이다.

실제 엔진 등록 직후의 catalog 실패 주입과 설치 controller 결합은 아직 제공하지 않는 UX-04 경로이므로 NOT_RUN이다. 이번 실패 검사와 보존 근거는 fixture/저장소 범위다. 새 package의 실제 설치·서비스·네트워크 인수, 다른 OS 계정으로 권한 교차 실행, logout/sleep·crash 내구성, 작성자가 아닌 사람의 사용성 과업도 미실행이다. 기존 RC.3 package PASS를 새 CLI/package의 PASS로 재사용하지 않는다. UX-07·전체 G1-M/J5와 안전 제거는 계속 후속이다.
