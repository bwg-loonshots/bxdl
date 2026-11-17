# LNX-01 Linux 기반 구현·회귀 기록

- 일자: 2026-11-16
- 상태: **코드 구현·자동 검사 PASS / 실제 Linux 인수 NOT_RUN**
- 기준: [실제 Linux 우선 계획](../design/2026-11-16-linux-first-implementation-plan.md), [지원 범위](../docs/support-matrix.md).
- 실행 host: macOS 26.6.2, Darwin 25.6.0 arm64. Rust/Cargo 1.86.0.
- Linux 기준: Ubuntu 24.04 LTS·x86_64/glibc·systemd, sudo/SSH 가능 전제로 구현. 실제 Linux 호스트에 접속하거나 OS·권한을 관측한 결과는 아니다.

## 구현

1. 독립 관리 CLI의 Linux bundle 생성·검증·설치·업데이트·복귀 경로를 추가했다. native Linux GNU/x86_64 빌더, ELF64/glibc interpreter 검사, 다른 target의 설치·후보 실행·복귀 거절을 포함한다. Mac의 기존 RC.3/RC.4 실행기 지원 조건을 유지한다.
2. Linux 관리 CLI 기본 저장소는 `~/.local/share/BXDL-CLI`, 진입점은 `~/.local/bin/bxdl`이다. 노드 이름·설치 준비의 사용자 목록은 `~/.local/share/BXDL`을 사용한다. `--root`/`--bin-dir`/`--home`은 용도가 다르고 XDG 환경변수로 자동 변경하지 않는다. 빈 목록 조회는 폴더를 만들지 않는다.
3. `bxdl host check`는 Ubuntu/systemd 기준을 읽기 전용으로 관측한다. 기준 일치도 노드 인수 성공이 아니며 `linuxServiceControl=NOT_IMPLEMENTED`, `engineAcceptance=NOT_RUN`, `serviceMutations=0`을 명시한다. WSL은 후속 검증으로 분류한다.
4. systemd unit·InvocationID·PID·cgroup·종료 metadata의 제한된 파서를 추가했다. 아직 unit 생성·서비스 start/stop 기능이 없으며 OS 종료 metadata만으로 엔진의 정상 종료를 인정하지 않는다.
5. Linux amd64/glibc·Java21 runtime inventory의 오프라인 staging을 추가했다. Linux lock의 `platform.libc=glibc`를 요구하며 기존 Mac lock 형식은 유지한다. 선정 Linux JRE 공급물이나 실행 결과를 새로 고정하지 않았다.

기존 `bxdl setup` 제품 초안은 Linux에서 `--workspace`가 필요하다. 이름 기반 준비 명령은 새 사용자 목록 기본값을 사용한다. 자동완성은 zsh만 지원하고 bash/fish는 후속이다.

## 검증

| 검사 | 실제 결과·의미 |
| --- | --- |
| `make check` | PASS — fmt·Clippy, Rust623 PASS/0FAIL, Python168개 중166 PASS·2기존 opt-in SKIP |
| `make check-linux` | PASS — `cargo check --locked --all-targets --target x86_64-unknown-linux-gnu`; Linux 링크·실행 아님 |
| 현재 Mac CLI 직접 실행 | version의 target은 darwin-arm64. host check는 INCOMPLETE/HOST_PLATFORM_UNSUPPORTED·exit3, 서비스 변경0·엔진NOT_RUN. host check 옵션 자동완성 PASS |
| 문서·diff 검사 | PASS — 상대 링크 대상과 `git diff --check` 확인 |
| 실제 Linux 관리 CLI 배포 인수 | NOT_RUN — 두 배포본과 bootstrap 입력이 없는 opt-in 시험은 SKIP이며 실제 설치 PASS가 아님 |
| 실제 Linux JRE/engine·설치·systemd·사용자 과업 | NOT_RUN |

초기 전체 검사에서 Clippy lint 3건을 수정했고, 다음 실행에서 신규 자동완성 테스트가 내부 프로토콜에 실행 파일 이름을 잘못 포함해 실패한 것을 고쳤다. 해당 자동완성 테스트 16개 재검사는 PASS했다. 이후 기본 catalog 경로 보완까지 포함한 전체 검사를 별도로 실행했다. 합성 ELF·systemd fixture나 Mac 회귀 PASS는 Linux 실물 인수로 확대하지 않는다.

커밋 전 공개 runtime lock 스키마도 Rust 구현과 같은 Mac/Linux 조합으로 정렬했다. JSON Schema 2020-12 검증에서 기존 Mac·Linux lock 입력 2개 허용, libc 누락·musl·ARM64 Linux·Mac에 libc 추가의 4개 입력 거절을 확인했다. 이 스키마 정렬은 실행 코드나 아래 source140파일 digest를 바꾸지 않는다.

## Source와 기존 근거

- base commit: `62f138941f0650a085e88e08d7db057d228a34b7`
- 작업 source: `DIRTY_UNCOMMITTED`, branch `codex/linux-native-foundation`. 새 정식 배포본의 source pin이 아니다.
- 구현·테스트·제작 도구 140파일의 내용 목록 SHA-256: `676ea715b4df6edf5d7e05bf835002d4f494476e59077c239da08377134443a8`. 정렬한 `파일 SHA-256  상대경로\n` 목록의 SHA-256이며 문서는 이 목록에 포함하지 않는다.
- Cargo.lock SHA-256: `2bd2330a367d957e37b5536ede4c76cbe9365b7d2933f8ba2416cc62c55decfc`.
- 기존 [RC4-06 결과 JSON](./2026-11-16-rc4-macos-network.json) SHA-256 `4c6bd3fd87b1e7aae1921ea3b0cffc9f893bbca09518358a5f68837fedc9fae8`은 그대로다. 이번 코드 검사를 과거 고정 runtime 인수 결과에 소급하지 않는다.

비공개 로컬 원자료는 다음 이름과 digest로 보관했다. 원자료의 존재나 자동 검사 PASS는 실제 Linux 배포 인수를 의미하지 않는다.

| 원자료 | SHA-256 |
| --- | --- |
| `bxdl-linux-foundation-check-complete.log` | `d7485a3ddab5080cab05466ac22f56c8258d0c8c707eecea5c66fa77163b0f22` |
| `bxdl-linux-foundation-linux-check-complete.log` | `188801a4c5ddfd73f0c4a0888bb6d93537ebe6bdcd8ca70efa68fbc25a98ab8b` |
| `bxdl-linux-foundation-cli-smoke.json` | `04e9cf4d598a498684dbf10cbb4beb91efd8e96d2000dff639f661f3616f5256` |
| `bxdl-linux-foundation-source.json` | `693d121bfaca2e3d09db388e71c7a0093e64fb938068c5102c04d7a10c54254d` |

## 다음 단계

LNX-02에서 Linux JRE 전체 공급물·inventory와 고정 RC.4 JAR 조합을 확인하고 Linux runtime identity를 새로 고정한다. LNX-03에서 systemd system service·instance별 비root UID·root 제어 기록과 정상 종료 판정을 구현한다. LNX-04 실제 단일노드 인수 뒤 LNX-05 테스터 과업을 시작하고, 서버·네트워크 인수와 WSL2 검증은 별도 단계로 진행한다. NIGO 공유 checkout은 수정하지 않았다.
