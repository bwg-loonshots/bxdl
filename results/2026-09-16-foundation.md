# Foundation 로컬 검증 — 2026-09-16

## 대상·환경

- BXDL 최초 커밋 전 작업 트리, CLI `0.1.0-dev` / revision `development`.
- Go 1.27.1, Darwin arm64, 표준 라이브러리만 사용. Go archive SHA는 docs/support-matrix.md에 기록.
- 실제 NIGO artifact·JRE·공급 계약 없음. tests의 engine/runtime bytes와 Ed25519 key는 test-owned 임시 fixture다.
- 현재 Linux 실행 환경 없음. Linux binary는 cross-build만 수행했다.

## 실행과 결과

아래 `<go>`는 공식 Go 1.27.1 실행파일이다. cache/modcache는 작업 전용 폴더를 사용하고 `GOTOOLCHAIN=local`로 추가 toolchain 다운로드를 막았다.

```text
gofmt -l cmd internal                         PASS (최종 출력 없음)
make GO=<go> check                            PASS
  go vet ./...                               PASS
  go test -race ./...                         PASS
    bxdl/internal/artifact                   PASS
    bxdl/internal/cli                        PASS
    bxdl/internal/config                     PASS
make GO=<go> build                            PASS (Darwin arm64)
make GO=<go> build-linux                      PASS (CGO=0, Linux amd64)
```

테스트 함수 39개와 그 하위 사례를 실행했다. 주요 근거는 외부 Ed25519 key·서명 원문·잘못된 key·payload 변조, unsigned 명시 opt-in, tar/gzip/link/traversal/중복·누락 inventory/필드·한도·key 형식, expected hash mismatch, 결정적 반복 조립, 입력 변화 감지, 기존 output 보존이다. 설정에서는 duplicate/unknown 필드·broadcast 주소·비밀정보 canary·경로/권한/symlink·비변경·누락 상태를 확인했다.

CLI 통합은 public `Run` 진입점으로 임시 서명 package build→verify, 잘못된 key/변조 거부, output 충돌 보존, config→preflight를 연결했다. 모든 로컬 metadata가 PASS여도 preflight는 INCOMPLETE/exit 5이고, 로컬 mode 실패는 exit 4다. JSON stdout은 문서 하나이고 stderr는 비어 있으며 비밀값·참조 경로를 노출하지 않는다.

교차 검토에서 빈 선택 파일의 `size` 누락과 limited broadcast 허용을 재현한 뒤 수정했고 해당 회귀 테스트도 위 full race run에 포함했다.

### 빌드된 native CLI 실행

| 명령 | 실제 exit / reasonCode | 의미 |
| --- | --- | --- |
| `bxdl version --json` | 0 / VERSION | CLI identity·NOT_DELIVERED·NOT_CHECKED 출력 |
| `bxdl config validate --file config/examples/instance.development.json --json` | 0 / PRODUCT_CONFIG_VALIDATED | 제품 설정 구조만 통과 |
| `bxdl preflight --config config/examples/instance.development.json --json` | 4 / LOCAL_CHECK_FAILED | 예시에 실제 key/chain 파일이 없으므로 예상한 실패 |
| `bxdl start --instance demo --json` | 4 / CAPABILITY_NOT_IMPLEMENTED | engine/service 작업 없이 미지원 거부 |

위 native CLI 4개 결과도 단일 JSON stdout·빈 stderr를 검사했다.

### 로컬 생성 binary SHA-256

| 파일 | hash |
| --- | --- |
| `bin/bxdl` | `351e83965ed37ea413dfc11651688ddf16d1a65af0794dd0be5f34ce14ef4066` |
| `dist/bxdl-linux-amd64` | `e3bb5ccf843ab3671844676c051e84ef3b4758e1c984f8bc30123f77cd3d80a2` |

`file`로 각각 Mach-O arm64 / statically linked ELF x86-64임을 확인했다. 위 binary는 로컬 개발 결과로 `.gitignore` 대상이며 공식 release artifact가 아니다. source snapshot 또는 toolchain을 변경해 재빌드하면 새 hash를 기록한다.

## 미실행

Linux에서 CLI 실행, 실제 JRE/native load, NIGO init/cold/start/stop, systemd 설치·재부팅·복구, 동일 DB/WAL 재시작, 4-validator mTLS/finality, offline 설치 인수, GitHub Actions 원격 실행, release 발행은 수행하지 않았다. 로컬 race 테스트와 cross-build가 이 항목의 대체 근거가 되지 않는다.

[작업별 상태와 다음 조건](../docs/implementation-status.md)을 따른다.
