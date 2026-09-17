# Mac development 패키지 설치

`install`은 macOS Apple Silicon에서 검증된 패키지 파일을 **새 폴더**에 설치한다. 폴더는 사용자 범위로 명시하고 부모는 미리 존재해야 한다. 현재는 setup의 설정 초안과 독립된 명령이다.

```bash
bxdl install ./bxdl-development.tar.gz \
  --destination "$HOME/Library/Application Support/BXDL/releases/candidate-01" \
  --public-key ./trusted-release-public.pem --json
```

서명 없는 개발 시험 자료에만 `--public-key` 대신 `--allow-unsigned-development`를 쓴다. 서명 자료에는 외부 신뢰 key가 반드시 필요하다. root/sudo를 요구하지 않는다. 설치 대상의 OS/CPU와 실제 host가 Mac arm64여야 하며 Linux 파일 설치는 후속이다.

설치 전에 manifest·서명·platform·Mac 경로 별칭/파일-디렉터리 충돌을 검사한다. 새 대상 폴더를 0700으로 독점 생성하고, 기존 verifier가 읽는 동일 payload bytes를 파일에 쓴다. 파일은 검사 중 0600이며 전체 hash·inventory·tar EOF·gzip CRC/trailing 검증이 끝난 뒤 manifest mode를 적용한다. 원본 manifest.json과 존재하는 manifest.sig도 설치 폴더에 남긴다.

완료 증거는 마지막에 기록하는 `.bxdl-install.json`이다. outcome=INSTALLED, archive/manifest hash, authenticity, 전체 manifest·파일/byte 수를 담으며 `engineValidation=NOT_CHECKED`, `lifecycle=NOT_PERFORMED`다. 이 receipt는 파일 설치 결과이며 NIGO의 instance journal·데이터 초기화·서비스 등록·정상 실행 증거가 아니다. 같은 UID가 수정할 수 있는 파일이므로 재실행 시 신뢰 루트로 취급하지 않는다.

payload 검증 실패나 기록 전 강제 중단은 receipt가 없는 부분 폴더를 남긴다. 마지막 receipt 공개·동기화 경계에서 I/O가 실패하면 INSTALL_COMMIT_UNCERTAIN/UNKNOWN/exit 6이며 receipt가 남았는지 확인해야 한다. 결과 출력 실패(exit 7)도 파일 설치 자체가 이미 끝났을 수 있다. 명령을 반복해 덮어쓰거나 자동 resume·삭제하지 않는다. 남은 폴더를 직접 확인하고 다른 새 경로를 지정한다. 파일이 보이거나 실행 권한이 있다는 사실만으로 설치 완료를 판단하지 않는다. 기존 대상은 빈 폴더여도 거부한다. symlink 경로를 사용하지 않으며 같은 UID의 악의적인 동시 변경에 대한 강한 보안 격리를 제공하지 않는다.

이후 `engine inspect`로 선택한 JAR/JRE와 외부 lock을 확인하고, 별도 NIGO node.json을 준비해 `engine preflight`를 실행할 수 있다. 설치된 launcher의 실행 가능성, 정식 JRE 공급·NOTICE/SBOM, Apple signing/notarization, launchd와 G1-M 전체 인수는 별도다. [엔진 검사 가이드](./engine.md)를 따른다.
